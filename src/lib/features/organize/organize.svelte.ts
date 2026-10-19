// Organizing a document's pages (section 6.7): the state behind the grid shown in place of
// the viewer. The pages are the document's own, so there is nothing to apply: every move,
// turn and deletion is an operation on the document at once, one undo step each (section
// 7). This holds only what the grid adds: the selection, the picture size, files dragged
// over it from another program, and pages being extracted into a new document.

import { GridSelection } from '#lib/components/gridselection.svelte.ts';
import { GridZoom } from '#lib/components/gridzoom.svelte.ts';
import { refuseRestricted } from '#lib/features/unprotect/actions.ts';
import { cancelMerge, extractPages, toAppError, type MergeProgress } from '#lib/ipc/index.ts';
import { app } from '#lib/stores/app.svelte.ts';
import type { DocTab } from '#lib/stores/doc.svelte.ts';

import { notifyDeleted } from './notify.ts';
import { deleteQuestion, extractedMessage, nudgeGap } from './pages.ts';

/** A page in the grid: its key, which it keeps wherever it goes, and its place now. */
export interface GridPage {
	key: number;
	index: number;
}

export class OrganizeState extends GridSelection {
	/** The size of the pictures (the bar's buttons, the keys, the wheel, a pinch). */
	readonly zoom = new GridZoom(() => app.settings?.smoothZoom !== false);
	/** Files from another program are dragged over the window, here (viewport pixels). */
	filePoint = $state<{ x: number; y: number } | null>(null);
	/** The gap the grid shows for them, or null while they are not over the grid. */
	fileGap: number | null = null;
	/** The gap files were dropped into, until they are opened. */
	private droppedAt: number | null = null;
	/** Pages are being extracted into a new document: how far that is, once it has started. */
	extracting = $state<{ progress: MergeProgress | null; stopping: boolean } | null>(null);
	/** A page for the grid to bring into view: it does when this changes. */
	reveal = $state<{ key: number } | null>(null);
	readonly tab: DocTab;

	/** The pages, in order. (Through methods: `tab` is set in the constructor, after these.) */
	items = $derived.by(() => this.pages());
	private places = $derived.by(() => this.placesByKey());
	/** Edits run one after another: each works out its pages from where they are when it
	 * runs, so keys held down (a nudge repeated) cannot overtake each other. */
	private queue: Promise<unknown> = Promise.resolve();

	constructor(tab: DocTab) {
		super();
		this.tab = tab;
		this.show(tab.currentPage, false);
	}

	protected keys(): number[] {
		return [...this.tab.pageKeys];
	}

	private pages(): GridPage[] {
		return this.tab.pageKeys.map((key, index) => ({ key, index }));
	}

	private placesByKey(): Map<number, number> {
		return new Map(this.tab.pageKeys.map((key, index) => [key, index]));
	}

	/** The pages can be moved, turned and deleted: the document allows it, and it is not
	 * open for reading only. */
	get editable(): boolean {
		return this.tab.flags.canAssemble;
	}

	/** The places of the pages with these keys, in page order. */
	indexes(keys: Iterable<number>): number[] {
		const out: number[] = [];
		for (const key of keys) {
			const index = this.places.get(key);
			if (index !== undefined) out.push(index);
		}
		return out.sort((a, b) => a - b);
	}

	/** Brings a page into view with the keyboard focus on it; `select` selects it alone. */
	show(index: number, select = true) {
		const key = this.tab.pageKeys[Math.max(0, Math.min(index, this.tab.pageKeys.length - 1))];
		if (key === undefined) return;
		if (select) this.select(key, 'single');
		else this.focusKey = key;
		this.reveal = { key };
	}

	/** Shows pages that have just arrived (inserted from a file), all selected, with the
	 * first of them in view. */
	showRange(first: number, count: number) {
		const keys = this.tab.pageKeys.slice(first, first + count);
		if (!keys.length) return;
		this.setSelection(keys, keys[0]!);
		this.reveal = { key: keys[0]! };
	}

	/** The page that "this page" means in the grid (a new label range from this page): the
	 * first selected one, or the one with the keyboard focus. */
	get anchorPage(): number {
		const focused = this.focusKey === null ? undefined : this.places.get(this.focusKey);
		return this.indexes(this.selected)[0] ?? focused ?? 0;
	}

	/** Where pages from a file go unless a gap is pointed at: after the last selected
	 * page, or after the page with the keyboard focus. */
	insertGap(): number {
		const selected = this.indexes(this.selected);
		const focused = this.focusKey === null ? undefined : this.places.get(this.focusKey);
		return (selected[selected.length - 1] ?? focused ?? this.tab.pageCount - 1) + 1;
	}

	/** The document's pages changed (an edit here, an undo, a reload): pages that are gone
	 * leave the selection, and the focus goes to a page that is there. */
	pagesChanged() {
		const places = new Set(this.tab.pageKeys);
		if (this.selected.some((key) => !places.has(key))) this.selected = this.selected.filter((key) => places.has(key));
		if (this.focusKey === null || !places.has(this.focusKey)) {
			this.focusKey = this.tab.pageKeys[Math.min(this.tab.currentPage, this.tab.pageKeys.length - 1)] ?? null;
		}
	}

	private run(edit: () => Promise<unknown>) {
		this.queue = this.queue.then(edit, edit).catch(() => {});
	}

	/** Says why the pages can't be changed, if they can't. */
	private refused(): boolean {
		if (this.editable) return false;
		if (app.refuseReadOnly(this.tab)) return true;
		refuseRestricted(this.tab, 'This document’s security settings don’t allow changing its pages.');
		return true;
	}

	move(keys: ReadonlySet<number>, gap: number) {
		this.run(async () => {
			const pages = this.indexes(keys);
			if (!pages.length || this.refused()) return;
			await app.apply(this.tab, { kind: 'movePages', pages, to: Math.min(gap, this.tab.pageCount) });
		});
	}

	nudge(direction: 1 | -1) {
		this.run(async () => {
			const pages = this.indexes(this.selected);
			const to = nudgeGap(pages, this.tab.pageCount, direction);
			if (to === null || this.refused()) return;
			await app.apply(this.tab, { kind: 'movePages', pages, to });
		});
	}

	rotate(keys: ReadonlySet<number>, degrees: number) {
		this.run(async () => {
			const pages = this.indexes(keys);
			if (!pages.length || this.refused()) return;
			await app.apply(this.tab, { kind: 'rotatePages', pages, degrees });
		});
	}

	/** Deletes the pages after asking (section 6.7), then says what went with them. */
	remove(keys: ReadonlySet<number>) {
		this.run(async () => {
			const tab = this.tab;
			const pages = this.indexes(keys);
			if (!pages.length || this.refused()) return;
			if (pages.length >= tab.pageCount) {
				app.notify({
					kind: 'info',
					message: 'A document needs at least one page.',
					suggestion: 'Leave a page unselected to delete the others.'
				});
				return;
			}
			if ((await app.ask(deleteQuestion(pages, tab.displayLabels))) !== 'delete') return;
			const change = await app.apply(tab, { kind: 'deletePages', pages });
			if (!change?.deleted) return;
			// As in a file list: the page after the deleted ones is selected.
			this.show(Math.min(pages[0]!, tab.pageCount - 1));
			notifyDeleted(change.deleted, () => void app.saveAs(tab, true));
		});
	}

	// ----- pages into a new document -----

	/** Copies the pages with these keys into a new document, which opens in a tab of its
	 * own with its pages as a grid, not saved yet (section 6.7). This document keeps them,
	 * and is not changed. */
	extract(keys: ReadonlySet<number>) {
		this.run(async () => {
			const tab = this.tab;
			const pages = this.indexes(keys);
			if (!pages.length || this.extracting) return;
			if (!tab.flags.canCopy) {
				refuseRestricted(tab, 'This document’s security settings don’t allow copying its pages into another file.');
				return;
			}
			this.extracting = { progress: null, stopping: false };
			try {
				const outcome = await extractPages(tab.id, pages, (progress) => {
					if (this.extracting) this.extracting.progress = progress;
				});
				app.handleOpenResults([outcome.opened]);
				// The new document's pages are what was asked for: they show as a grid too.
				const opened = outcome.opened;
				const made = opened.kind === 'opened' ? app.tabs.find((t) => t.id === opened.document.id) : undefined;
				if (made && !made.organize) {
					made.organize = new OrganizeState(made);
					made.organize.zoom.zoomTo(this.zoom.thumbTarget);
				}
				app.notify({ kind: 'info', ...extractedMessage(outcome.report) }, 12000);
			} catch (e) {
				const error = toAppError(e);
				if (error.code === 'cancelled') app.notify({ kind: 'info', message: error.message });
				else app.showError(error);
			} finally {
				this.extracting = null;
			}
		});
	}

	/** Stops the extraction in progress; it ends without writing a file. */
	stopExtract() {
		if (!this.extracting || this.extracting.stopping) return;
		this.extracting.stopping = true;
		void cancelMerge().catch(() => {});
	}

	// ----- files dragged in from another program -----

	fileOver(x: number, y: number) {
		this.filePoint = { x, y };
	}

	fileLeft() {
		this.filePoint = null;
		this.fileGap = null;
	}

	/** The files were let go: their pages go into the gap shown, once they are opened. */
	fileDropped() {
		this.droppedAt = this.fileGap;
		this.fileLeft();
	}

	/** The gap for the files dropped last (once): the one they were dropped into, else
	 * the usual one. */
	takeDrop(): number {
		const at = this.droppedAt ?? this.insertGap();
		this.droppedAt = null;
		return at;
	}

	/** Resolves once every edit started so far has run. */
	settled(): Promise<unknown> {
		return this.queue;
	}
}
