// The Compare view's state (section 6.13), as ABBYY FineReader's Compare Documents has it:
// two slots, each holding a document opened in Rust as a source (not a tab) and shown in a
// pane of its own, read only; the options; the comparison in progress; and its result:
// the differences, which of them are ignored, the one selected, and the places the two
// documents line up at, which the panes scroll together by.

import {
	cancelUnlock,
	closeDocument,
	compareForget,
	compareRun,
	compareSaveAnnotated,
	compareSaveReport,
	compareSettings,
	compareStop,
	openCompareSource,
	openCompareSourceOf,
	toAppError,
	unlockDocument,
	type AppError,
	type CompareAnchor,
	type CompareProgress,
	type CompareSettings,
	type Difference,
	type DocumentInfo,
	type OpenResult,
	type ScrollMode
} from '#lib/ipc/index.ts';
import type { ZoomMode } from '#lib/features/viewer/zoom.ts';
import { zoomKey } from '#lib/features/split/sync.ts';
import type { DialogRequest, Toast } from '#lib/stores/app.svelte.ts';
import { DocTab } from '#lib/stores/doc.svelte.ts';

import {
	copyText,
	identical,
	mapPlace,
	markAt,
	marksByPage,
	otherSide,
	placeIn,
	sections,
	type CompareMark,
	type Side
} from './compare.ts';

/** What a pane's view (`DocTab.comparing`) asks of the Compare view. */
export interface ComparePane {
	readonly side: Side;
	/** The marks page `page` shows. */
	marks(page: number): readonly CompareMark[];
	/** The difference selected, whose marks stand out. */
	selected(): number | null;
	/** A press at a point of a page (page points): on a mark, selects its difference. */
	press(page: number, x: number, y: number): void;
	/** The pane was scrolled, not by following the other. */
	scrolled(): void;
}

/** What the Compare view needs from the rest of the app. */
export interface CompareHost {
	notify(toast: Omit<Toast, 'id'>, timeoutMs?: number): void;
	showError(error: AppError): void;
	ask(request: Omit<DialogRequest, 'resolve'>): Promise<string>;
	askPassword(token: number, name: string, retry: boolean, answer: (password: string | null) => Promise<void>): void;
	/** The documents open in tabs, to pick one from. */
	openDocuments(): { id: number; name: string; dirty: boolean; untitled: boolean }[];
	/** Saves the document in tab `id`; false if that failed or was cancelled. */
	saveTab(id: number): Promise<boolean>;
	/** How documents are laid out as they open (Settings). */
	scrollMode(): ScrollMode;
}

export interface CompareResult {
	differences: Difference[];
	anchors: CompareAnchor[];
	oldWords: number;
	newWords: number;
	/** The documents compared: their ids, old then new. */
	pair: [number, number];
	when: Date;
}

const DEFAULT_SETTINGS: CompareSettings = { punctuation: true, oneLetter: true, margins: true, mode: 'automatic' };

/** After a jump to a difference, how long the panes' scrolling leads nothing (ms): both are
 * placed on it, and neither follows the other meanwhile. */
const QUIET_MS = 700;

export class CompareState {
	/** The first document (old) and the second (new), as views of their own. */
	views = $state<[DocTab | null, DocTab | null]>([null, null]);
	/** A slot whose file is being picked or opened. */
	opening = $state<[boolean, boolean]>([false, false]);
	settings = $state<CompareSettings>({ ...DEFAULT_SETTINGS });
	running = $state(false);
	stopping = $state(false);
	progress = $state<CompareProgress | null>(null);
	result = $state.raw<CompareResult | null>(null);
	/** Differences removed from the list (and the saved copies and report) by number. */
	ignored = $state.raw<ReadonlySet<number>>(new Set());
	/** Ignored differences are listed, dimmed, so they can be put back. */
	showIgnored = $state(false);
	/** The list in FineReader's groups. */
	grouped = $state(true);
	/** The difference selected: marked in both panes. */
	current = $state<number | null>(null);
	/** The rows selected in the list (Ctrl and Shift add), for ignoring or copying. */
	picked = $state.raw<ReadonlySet<number>>(new Set());
	/** The panel's tab. */
	panel = $state<'compare' | 'differences'>('compare');
	/** The panes scroll together (on from the start, as in FineReader). */
	locked = $state(true);

	/** Each pane's marks by page, for the differences not ignored. */
	readonly marks = $derived({
		old: this.result ? marksByPage(this.result.differences, 'old', this.ignored) : new Map<number, CompareMark[]>(),
		new: this.result ? marksByPage(this.result.differences, 'new', this.ignored) : new Map<number, CompareMark[]>()
	});
	/** The list as shown, section by section. */
	readonly sections = $derived(
		this.result ? sections(this.result.differences, this.grouped, this.ignored, this.showIgnored) : []
	);
	/** The differences listed, in the order they are listed. */
	readonly listed = $derived(this.sections.flatMap((s) => s.ids.filter((id) => !this.ignored.has(id))));
	readonly ready = $derived(this.views[0] !== null && this.views[1] !== null);

	// Scrolling together: where each pane was, the pane leading, and how far the other is
	// from where the comparison's anchors would put it.
	private places: Record<Side, number> = { old: 0, new: 0 };
	private lead: Side | null = null;
	private offset = 0;
	private rebase = true;
	private quietUntil = 0;
	private zooms: Partial<Record<Side, string>> = {};
	private settingsRead: Promise<void>;

	constructor(private readonly host: CompareHost) {
		this.settingsRead = compareSettings()
			.then((s) => {
				this.settings = s;
			})
			.catch(() => {});
	}

	view(side: Side): DocTab | null {
		return this.views[side === 'old' ? 0 : 1];
	}

	slotOf(side: Side): 0 | 1 {
		return side === 'old' ? 0 : 1;
	}

	// ----- the documents -----

	/** Asks for a file for a slot in the Open dialog. */
	async open(slot: 0 | 1) {
		if (this.running || this.opening[slot]) return;
		this.setOpening(slot, true);
		try {
			const result = await openCompareSource(slot === 1);
			if (result) await this.opened(slot, result);
		} catch (e) {
			this.host.showError(toAppError(e));
		} finally {
			this.setOpening(slot, false);
		}
	}

	/** Puts the file of a document open in a tab in a slot, as saved: unsaved changes are
	 * saved first, if the user agrees. */
	async openTab(slot: 0 | 1, id: number) {
		if (this.running || this.opening[slot]) return;
		const doc = this.host.openDocuments().find((d) => d.id === id);
		if (!doc) return;
		if (doc.dirty || doc.untitled) {
			const choice = await this.host.ask({
				title: 'Save your changes first?',
				message: `${doc.name} has unsaved changes.`,
				detail: 'Compare reads the file as it is saved, so save them first.',
				buttons: [
					{ id: 'save', label: 'Save', primary: true },
					{ id: 'cancel', label: 'Cancel' }
				],
				cancel: 'cancel'
			});
			if (choice !== 'save' || !(await this.host.saveTab(id))) return;
		}
		this.setOpening(slot, true);
		try {
			await this.opened(slot, await openCompareSourceOf(id));
		} catch (e) {
			this.host.showError(toAppError(e));
		} finally {
			this.setOpening(slot, false);
		}
	}

	/** Files dropped on the view, opened as sources: two fill both slots; one goes to the
	 * slot it was dropped on, or the first empty one. */
	async dropped(results: OpenResult[], slot: 0 | 1 | null) {
		if (this.running) {
			this.discard(results);
			return;
		}
		if (results.length >= 2) {
			await this.opened(0, results[0]!);
			await this.opened(1, results[1]!);
			this.discard(results.slice(2));
			return;
		}
		const one = results[0];
		if (!one) return;
		const to = slot ?? (this.views[0] === null ? 0 : this.views[1] === null ? 1 : 0);
		await this.opened(to, one);
	}

	private discard(results: OpenResult[]) {
		for (const r of results) {
			if (r.kind === 'opened') void closeDocument(r.document.id).catch(() => {});
			else if (r.kind === 'needsPassword') void cancelUnlock(r.token).catch(() => {});
		}
	}

	private async opened(slot: 0 | 1, result: OpenResult) {
		switch (result.kind) {
			case 'opened':
				this.place(slot, result.document);
				return;
			case 'needsPassword':
				this.host.askPassword(result.token, result.name, result.retry, async (password) => {
					if (password === null) {
						await cancelUnlock(result.token).catch(() => {});
						return;
					}
					try {
						await this.opened(slot, await unlockDocument(result.token, password));
					} catch (e) {
						this.host.showError(toAppError(e));
					}
				});
				return;
			case 'failed':
				this.host.notify(
					{ kind: 'error', message: `${result.name}: ${result.error.message}`, suggestion: result.error.suggestion },
					12000
				);
				return;
			case 'alreadyOpen':
				// Sources are never matched to tabs.
				return;
		}
	}

	/** Shows a document in a slot, in place of what was there; what was compared goes. */
	private place(slot: 0 | 1, info: DocumentInfo) {
		const side: Side = slot === 0 ? 'old' : 'new';
		const view = new DocTab(info, this.host.scrollMode());
		view.comparing = this.pane(side);
		const views: [DocTab | null, DocTab | null] = [...this.views];
		const before = views[slot];
		views[slot] = view;
		this.clearResult();
		this.views = views;
		if (before) this.closeView(before);
	}

	/** Empties a slot. */
	remove(slot: 0 | 1) {
		if (this.running) return;
		const view = this.views[slot];
		if (!view) return;
		this.clearResult();
		const views: [DocTab | null, DocTab | null] = [...this.views];
		views[slot] = null;
		this.views = views;
		this.closeView(view);
	}

	/** The first document becomes the second and the second the first. */
	swap() {
		if (this.running) return;
		const [a, b] = this.views;
		this.clearResult();
		if (a) a.comparing = this.pane('new');
		if (b) b.comparing = this.pane('old');
		this.views = [b, a];
	}

	private closeView(view: DocTab) {
		view.comparing = null;
		view.dispose();
		void closeDocument(view.id).catch(() => {});
	}

	private setOpening(slot: 0 | 1, on: boolean) {
		const opening: [boolean, boolean] = [...this.opening];
		opening[slot] = on;
		this.opening = opening;
	}

	/** The view of a pane: how its pages show the differences, and how it scrolls. */
	private pane(side: Side): ComparePane {
		return {
			side,
			marks: (page) => this.marks[side].get(page) ?? [],
			selected: () => this.current,
			press: (page, x, y) => {
				const id = markAt(this.marks[side].get(page) ?? [], x, y);
				this.lastPressed = side;
				if (id !== null) this.select(id, { reveal: 'other' });
			},
			scrolled: () => this.scrolled(side)
		};
	}

	// ----- comparing -----

	async run() {
		const [a, b] = this.views;
		if (!a || !b || this.running) return;
		await this.settingsRead;
		this.clearResult();
		this.running = true;
		this.stopping = false;
		this.progress = null;
		try {
			const outcome = await compareRun(a.id, b.id, { ...this.settings }, (p) => (this.progress = p));
			// A document replaced meanwhile: this result is for documents gone.
			if (this.views[0] !== a || this.views[1] !== b) return;
			this.result = { ...outcome, pair: [a.id, b.id], when: new Date() };
			this.panel = 'differences';
			this.rebase = true;
			const first = this.listed[0];
			if (first !== undefined) this.select(first);
		} catch (e) {
			const error = toAppError(e);
			if (error.code !== 'cancelled') this.host.showError(error);
		} finally {
			this.running = false;
			this.stopping = false;
			this.progress = null;
		}
	}

	stop() {
		const [a, b] = this.views;
		if (!this.running || !a || !b) return;
		this.stopping = true;
		void compareStop(a.id, b.id).catch(() => {});
	}

	private clearResult() {
		const result = this.result;
		if (result) void compareForget(result.pair[0], result.pair[1]).catch(() => {});
		this.result = null;
		this.ignored = new Set();
		this.picked = new Set();
		this.current = null;
		this.showIgnored = false;
		this.panel = 'compare';
	}

	// ----- the differences -----

	/** Selects a difference and brings it into view: in both panes, or (`other`) in the pane
	 * that was not pressed. With `add` (Ctrl) or `range` (Shift), rows join the selection. */
	select(id: number, options: { reveal?: 'both' | 'other' | 'none'; add?: boolean; range?: boolean } = {}) {
		const result = this.result;
		if (!result?.differences[id]) return;
		if (options.range && this.current !== null) {
			const order = this.sections.flatMap((s) => s.ids);
			const ends = [order.indexOf(this.current), order.indexOf(id)];
			const from = Math.min(...ends);
			const to = Math.max(...ends);
			if (from >= 0) this.picked = new Set(order.slice(from, to + 1));
		} else if (options.add) {
			const picked = new Set(this.picked);
			if (picked.has(id)) picked.delete(id);
			else picked.add(id);
			this.picked = picked;
		} else {
			this.picked = new Set([id]);
		}
		this.current = id;
		const reveal = options.reveal ?? 'both';
		if (reveal === 'none') return;
		const d = result.differences[id]!;
		this.quietUntil = performance.now() + QUIET_MS;
		this.rebase = true;
		for (const side of ['old', 'new'] as const) {
			const view = this.view(side);
			if (!view?.viewer) continue;
			if (reveal === 'other' && this.lastPressed === side) continue;
			const at = placeIn(d, side);
			view.viewer.reveal(at.page, at.rect, { smooth: true });
		}
	}

	/** The pane pressed last, which a click on a mark leaves where it is. */
	private lastPressed: Side | null = null;

	/** The next (1) or previous (-1) difference listed. */
	step(direction: 1 | -1) {
		const list = this.listed;
		if (list.length === 0) return;
		const at = this.current === null ? -1 : list.indexOf(this.current);
		const next = at < 0 ? (direction === 1 ? 0 : list.length - 1) : at + direction;
		const id = list[Math.min(Math.max(next, 0), list.length - 1)];
		if (id !== undefined && id !== this.current) this.select(id);
		else if (id !== undefined && at < 0) this.select(id);
	}

	/** Where the selected difference is in the list, and how many it lists. */
	readonly position = $derived.by(() => {
		const at = this.current === null ? -1 : this.listed.indexOf(this.current);
		return { at, count: this.listed.length };
	});

	/** Takes differences out of the list, the saved copies and the report; the selection
	 * goes to the next one listed. */
	ignore(ids: Iterable<number>) {
		const take = new Set(ids);
		if (take.size === 0) return;
		const list = this.listed;
		const at = this.current === null ? -1 : list.indexOf(this.current);
		this.ignored = new Set([...this.ignored, ...take]);
		this.picked = new Set([...this.picked].filter((id) => !take.has(id)));
		if (this.current !== null && take.has(this.current)) {
			const after = list.slice(at + 1).find((id) => !take.has(id)) ?? list.slice(0, at).reverse().find((id) => !take.has(id));
			this.current = null;
			if (after !== undefined) this.select(after);
		}
	}

	/** Ignores the differences the same as `id` (same kind, same text in both). */
	ignoreIdentical(id: number) {
		if (this.result) this.ignore(identical(this.result.differences, id));
	}

	/** Puts ignored differences back in the list. */
	restore(ids: Iterable<number>) {
		const back = new Set(ids);
		this.ignored = new Set([...this.ignored].filter((id) => !back.has(id)));
	}

	/** Copies differences as text (the rows selected, or `ids`). */
	async copy(ids: readonly number[]) {
		const result = this.result;
		const [a, b] = this.views;
		if (!result || !a || !b) return;
		const names = { old: a.name, new: b.name };
		const labels = { old: a.displayLabels, new: b.displayLabels };
		const text = ids
			.map((id) => result.differences[id])
			.filter((d): d is Difference => d !== undefined)
			.map((d) => copyText(d, names, labels))
			.join('\n\n');
		try {
			await navigator.clipboard.writeText(text);
		} catch {
			this.host.notify({ kind: 'error', message: 'The difference couldn’t be copied to the clipboard.' });
		}
	}

	// ----- saving -----

	private skip(): number[] {
		return [...this.ignored].sort((a, b) => a - b);
	}

	/** Saves a copy of one document with the differences marked and commented. */
	async saveAnnotated(side: Side) {
		const result = this.result;
		if (!result) return;
		try {
			const name = await compareSaveAnnotated(result.pair[0], result.pair[1], side, this.skip());
			if (name === null) return;
			const count = result.differences.length - this.ignored.size;
			this.host.notify({
				kind: 'info',
				message: `Saved ${name}, with ${count === 1 ? '1 difference' : `${count.toLocaleString()} differences`} marked.`,
				suggestion: `The document compared is unchanged.`
			});
		} catch (e) {
			this.host.showError(toAppError(e));
		}
	}

	/** Saves the report of the differences. */
	async saveReport() {
		const result = this.result;
		if (!result) return;
		const when = new Intl.DateTimeFormat(undefined, { dateStyle: 'long', timeStyle: 'short' }).format(result.when);
		try {
			const name = await compareSaveReport(result.pair[0], result.pair[1], this.skip(), this.grouped, when);
			if (name !== null) this.host.notify({ kind: 'info', message: `Saved the report ${name}.` });
		} catch (e) {
			this.host.showError(toAppError(e));
		}
	}

	// ----- scrolling and zooming together -----

	/** A pane scrolled, not by following the other: while the lock is on, the other goes
	 * where the comparison lines it up with this one, as far from there as it was when this
	 * pane began to lead. */
	private scrolled(side: Side) {
		const lead = this.view(side);
		const other = this.view(otherSide(side));
		if (!lead?.viewer) return;
		const now = lead.viewer.place();
		const before = this.places[side];
		this.places[side] = now;
		if (!this.locked || !other?.viewer || performance.now() < this.quietUntil) {
			this.rebase = true;
			return;
		}
		const anchors = this.result?.anchors ?? [];
		const map = (place: number) => mapPlace(anchors, place, side, lead.pageCount, other.pageCount);
		if (this.lead !== side || this.rebase) {
			this.lead = side;
			this.rebase = false;
			this.offset = other.viewer.place() - map(before);
		}
		const target = Math.min(Math.max(map(now) + this.offset, 0), other.pageCount);
		this.places[otherSide(side)] = target;
		other.viewer.follow(target);
	}

	setLocked(on: boolean) {
		this.locked = on;
		this.rebase = true;
	}

	/** A pane's zoom changed: the other takes it, a fit as a fit of its own. Run by the view
	 * as either pane's zoom changes. */
	zoomed() {
		const panes = (['old', 'new'] as const).map((side) => ({ side, view: this.view(side) }));
		const changed = panes.filter((p) => p.view && this.zooms[p.side] !== zoomKey(p.view.zoomMode, p.view.zoom));
		if (changed.length === 0) return;
		// Both changed (a pane just placed): the first leads.
		const source = changed[0]!;
		const view = source.view!;
		const key = zoomKey(view.zoomMode, view.zoom);
		for (const p of panes) {
			if (!p.view) continue;
			this.zooms[p.side] = key;
			if (p.side === source.side || zoomKey(p.view.zoomMode, p.view.zoom) === key) continue;
			void this.followZoom(p.view, view.zoom, view.zoomMode);
		}
	}

	private async followZoom(view: DocTab, zoom: number, mode: ZoomMode) {
		await view.viewer?.followZoom(zoom, mode);
		this.rebase = true;
	}

	/** Closes both documents and forgets the comparison. */
	async dispose() {
		this.stop();
		this.clearResult();
		const views = this.views.filter((v): v is DocTab => v !== null);
		this.views = [null, null];
		for (const view of views) {
			view.comparing = null;
			view.dispose();
		}
		await Promise.all(views.map((v) => closeDocument(v.id).catch(() => {})));
	}
}
