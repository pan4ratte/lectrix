// The Combine view's state (section 6.4): the files added, the pages of the result in
// order, the options, and the merge in progress. Files are opened in Rust as sources (not
// tabs) so their thumbnails render; nothing is written until the user combines, and then
// only to a new file.

import {
	cancelMerge,
	closeDocument,
	executeMerge,
	openMergeSources,
	planMerge,
	toAppError,
	type AppError,
	type BookmarkMode,
	type LabelMode,
	type MergeProgress,
	type MergeRequest,
	type OpenResult,
	type PageSize
} from '#lib/ipc/index.ts';
import type { DialogRequest, Toast } from '#lib/stores/app.svelte.ts';

import {
	History,
	movePages,
	nudgePages,
	rangeKeys,
	removePages,
	reportDetail,
	rotatePages,
	signedWarning,
	summary,
	type CombinePage
} from './pages.ts';

/** Distinct colors that tell files apart in the grid (`--lectrix-source-N` tokens). */
export const SOURCE_COLORS = 6;

export interface CombineSource {
	/** The source document's id in Rust. */
	id: number;
	name: string;
	path: string;
	pages: PageSize[];
	/** One label per page, or null without labels. */
	labels: string[] | null;
	revision: number;
	/** Holds a signature, which will not be valid in the combined file. */
	signed: boolean;
	/** Which `--lectrix-source-N` color marks its pages (1-based). */
	color: number;
}

/** What the Combine view needs from the rest of the app. */
export interface CombineHost {
	notify(toast: Omit<Toast, 'id'>, timeoutMs?: number): void;
	showError(error: AppError): void;
	ask(request: Omit<DialogRequest, 'resolve'>): Promise<string>;
	askPassword(token: number, name: string, retry: boolean): void;
	/** Saves the document in tab `id`; false if that failed or was cancelled. */
	saveTab(id: number): Promise<boolean>;
	/** Shows the combined file (its tab). */
	opened(result: OpenResult): void;
}

interface Snapshot {
	pages: CombinePage[];
	selected: number[];
}

export class CombineState {
	sources = $state<CombineSource[]>([]);
	pages = $state<CombinePage[]>([]);
	/** Keys of the selected pages. */
	selected = $state<number[]>([]);
	selectedSet = $derived(new Set(this.selected));
	/** The page with keyboard focus, also the anchor for Shift+click and Shift+arrows. */
	focusKey = $state<number | null>(null);
	bookmarks = $state<BookmarkMode>('nest');
	labels = $state<LabelMode>('keep');
	/** Set while combining, once the file name has been chosen. */
	progress = $state<MergeProgress | null>(null);
	running = $state(false);
	stopping = $state(false);
	/** Set once a merge succeeded and nothing changed since. */
	combinedAs = $state<string | null>(null);

	private anchor: number | null = null;
	private history = new History<Snapshot>();
	/** Bumped on every change, so undo availability is reactive. */
	private version = $state(0);
	private nextKey = 1;
	private colorCount = 0;

	canUndo = $derived.by(() => {
		void this.version;
		return this.history.canUndo;
	});
	canRedo = $derived.by(() => {
		void this.version;
		return this.history.canRedo;
	});
	/** Files with pages in the list, in the order they were added. */
	usedSources = $derived.by(() => {
		const used = new Set(this.pages.map((p) => p.source));
		return this.sources.filter((s) => used.has(s.id));
	});
	summary = $derived(summary(this.pages.length, this.usedSources.length));

	constructor(private readonly host: CombineHost) {}

	source(id: number): CombineSource | undefined {
		return this.sources.find((s) => s.id === id);
	}

	/** Pages of `source` in the list now. */
	pagesFrom(source: number): number {
		return this.pages.filter((p) => p.source === source).length;
	}

	// ----- adding files -----

	async addFiles() {
		try {
			this.addOpened(await openMergeSources());
		} catch (e) {
			this.host.showError(toAppError(e));
		}
	}

	/** Adds files opened as sources (from the dialog, a drop, or a password prompt). */
	addOpened(results: OpenResult[]) {
		const added: CombinePage[] = [];
		for (const result of results) {
			switch (result.kind) {
				case 'opened': {
					const doc = result.document;
					if (!doc.flags.canCopy) {
						this.host.notify(
							{
								kind: 'error',
								message: `${doc.name} doesn’t allow copying its pages into another document.`,
								suggestion: 'Its security settings forbid it. Ask its author for an unrestricted copy.'
							},
							12000
						);
						void closeDocument(doc.id).catch(() => {});
						break;
					}
					this.sources = [
						...this.sources,
						{
							id: doc.id,
							name: doc.name,
							path: doc.path,
							pages: doc.pages,
							labels: doc.labels,
							revision: doc.state.revision,
							signed: doc.flags.signed,
							color: (this.colorCount++ % SOURCE_COLORS) + 1
						}
					];
					added.push(...this.newPages(doc.id, doc.pages.map((_, i) => i)));
					break;
				}
				case 'needsPassword':
					this.host.askPassword(result.token, result.name, result.retry);
					break;
				case 'failed':
					this.host.notify(
						{ kind: 'error', message: `${result.name}: ${result.error.message}`, suggestion: result.error.suggestion },
						12000
					);
					break;
				case 'alreadyOpen':
					// Sources are never matched to tabs, so this does not happen.
					break;
			}
		}
		if (added.length) {
			this.change({ pages: [...this.pages, ...added], selected: [] });
		}
	}

	private newPages(source: number, pages: number[]): CombinePage[] {
		return pages.map((page) => ({ key: this.nextKey++, source, page, rotation: 0 }));
	}

	/** Puts a file's pages that are not in the list back, at the end, in their order. */
	restoreFile(id: number) {
		const source = this.source(id);
		if (!source) return;
		const present = new Set(this.pages.filter((p) => p.source === id).map((p) => p.page));
		const missing = source.pages.map((_, i) => i).filter((i) => !present.has(i));
		if (missing.length) this.change({ pages: [...this.pages, ...this.newPages(id, missing)] });
	}

	removeFile(id: number) {
		this.remove(new Set(this.pages.filter((p) => p.source === id).map((p) => p.key)));
	}

	// ----- editing the list (undoable) -----

	private change(next: Partial<Snapshot>) {
		this.history.push({ pages: this.pages, selected: this.selected });
		if (next.pages) this.pages = next.pages;
		if (next.selected) this.selected = next.selected;
		this.combinedAs = null;
		this.version++;
	}

	remove(keys: ReadonlySet<number>) {
		if (keys.size === 0) return;
		// Focus moves to the page after the removed ones, as in a file list.
		const index = this.pages.findIndex((p) => keys.has(p.key));
		const pages = removePages(this.pages, keys);
		this.change({ pages, selected: [] });
		const next = pages[Math.min(index, pages.length - 1)];
		this.focusKey = next?.key ?? null;
		if (next) this.selected = [next.key];
	}

	rotate(keys: ReadonlySet<number>, degrees: number) {
		if (keys.size === 0) return;
		this.change({ pages: rotatePages(this.pages, keys, degrees) });
	}

	move(keys: ReadonlySet<number>, gap: number) {
		const pages = movePages(this.pages, keys, gap);
		if (pages.every((p, i) => p === this.pages[i])) return;
		this.change({ pages });
	}

	nudge(direction: 1 | -1) {
		const pages = nudgePages(this.pages, this.selectedSet, direction);
		if (pages !== this.pages) this.change({ pages });
	}

	undo() {
		const previous = this.history.undo({ pages: this.pages, selected: this.selected });
		if (previous) this.restore(previous);
	}

	redo() {
		const next = this.history.redo({ pages: this.pages, selected: this.selected });
		if (next) this.restore(next);
	}

	private restore(snapshot: Snapshot) {
		this.pages = snapshot.pages;
		this.selected = snapshot.selected;
		this.combinedAs = null;
		this.version++;
	}

	// ----- selection -----

	/** Click: `single` selects one page, `toggle` (Ctrl) adds or removes it, `range` (Shift) selects from the anchor. */
	select(key: number, mode: 'single' | 'toggle' | 'range') {
		if (mode === 'range' && this.anchor !== null) {
			this.selected = rangeKeys(this.pages, this.anchor, key);
		} else if (mode === 'toggle') {
			this.selected = this.selectedSet.has(key)
				? this.selected.filter((k) => k !== key)
				: [...this.selected, key];
			this.anchor = key;
		} else {
			this.selected = [key];
			this.anchor = key;
		}
		this.focusKey = key;
	}

	selectAll() {
		this.selected = this.pages.map((p) => p.key);
	}

	// ----- combining -----

	async combine() {
		if (this.running || this.pages.length === 0) return;
		const used = this.usedSources.map((s) => s.id);
		const index = new Map(used.map((id, i) => [id, i]));
		const signed = this.usedSources.filter((s) => s.signed).map((s) => s.name);
		if (signed.length && (await this.host.ask(signedWarning(signed, false))) !== 'go') return;
		try {
			const plan = await planMerge(used);
			if (plan.missing.length) {
				const one = plan.missing.length === 1;
				this.host.notify(
					{
						kind: 'error',
						message: `${plan.missing.join(', ')} ${one ? 'was' : 'were'} moved or deleted.`,
						suggestion: `Remove ${one ? 'it' : 'them'} from the list, then add ${one ? 'it' : 'them'} again from the new place.`
					},
					12000
				);
				return;
			}
			if (plan.unsaved.length) {
				const one = plan.unsaved.length === 1;
				const choice = await this.host.ask({
					title: 'Unsaved changes',
					message: `${plan.unsaved.map((u) => u.name).join(', ')} ${one ? 'has' : 'have'} unsaved changes in ${one ? 'its tab' : 'their tabs'}.`,
					detail: 'Combining uses files as they are saved on disk.',
					buttons: [
						{ id: 'save', label: 'Save and combine', primary: true },
						{ id: 'saved', label: 'Use saved version' },
						{ id: 'cancel', label: 'Cancel' }
					],
					cancel: 'cancel'
				});
				if (choice === 'cancel') return;
				if (choice === 'save') {
					for (const unsaved of plan.unsaved) {
						if (!(await this.host.saveTab(unsaved.tab))) return;
					}
				}
			}
		} catch (e) {
			this.host.showError(toAppError(e));
			return;
		}

		const request: MergeRequest = {
			sources: used,
			pages: this.pages.map((p) => ({ source: index.get(p.source) ?? 0, page: p.page, rotation: p.rotation })),
			bookmarks: this.bookmarks,
			labels: this.labels
		};
		const done = this.summary;
		this.running = true;
		this.stopping = false;
		this.progress = null;
		try {
			const outcome = await executeMerge(request, (p) => {
				this.progress = p;
			});
			if (!outcome) return;
			this.combinedAs = outcome.name;
			this.host.opened(outcome.opened);
			const detail = reportDetail(outcome.report);
			this.host.notify(
				{ kind: 'info', message: `Combined ${done} into ${outcome.name}.`, suggestion: detail },
				detail ? 15000 : 8000
			);
		} catch (e) {
			const error = toAppError(e);
			if (error.code === 'cancelled') this.host.notify({ kind: 'info', message: error.message });
			else this.host.showError(error);
		} finally {
			this.running = false;
			this.stopping = false;
			this.progress = null;
		}
	}

	/** Stops the merge in progress; it ends without writing a file. */
	stop() {
		if (!this.running || this.stopping) return;
		this.stopping = true;
		void cancelMerge().catch(() => {});
	}

	/** Closes every source (when the Combine view closes). */
	async dispose() {
		const ids = this.sources.map((s) => s.id);
		this.sources = [];
		this.pages = [];
		await Promise.all(ids.map((id) => closeDocument(id).catch(() => {})));
	}
}
