// One open document: what Rust told us about it (DocModel), and each view of it in this
// window (DocTab: a tab, or a pane of tabs side by side), with how the user is viewing it.
// The document itself lives in Rust (section 7); this is only a view of it.

import {
	getPageText,
	logError,
	searchText,
	type Annotation,
	type DocumentChange,
	type DocumentFlags,
	type DocumentInfo,
	type DocumentState,
	type LabelRule,
	type Outline,
	type PageAdjustments,
	type PageAnnotations,
	type PageSize,
	type SaveResult,
	type ScrollMode,
	type TabHandoff,
	type ViewState
} from '#lib/ipc/index.ts';
import type { Box } from '#lib/features/annotations/geometry.ts';
import { lineBreaks } from '#lib/features/annotations/tools.ts';
import { pageDefaults } from '#lib/features/viewer/adjustments.svelte.ts';
import {
	normalizeAdjustments,
	pageFilter as filterOf,
	pageTone as toneOf,
	sameAdjustments
} from '#lib/features/viewer/adjustments.ts';
import type { OrganizeState } from '#lib/features/organize/organize.svelte.ts';
import { NavHistory, type ViewPosition } from '#lib/features/viewer/history.ts';
import {
	columnsOf,
	documentMode,
	isContinuous,
	normalizeRotation,
	type Rotation,
	type ViewMode
} from '#lib/features/viewer/layout.ts';
import { prepareText, type Caret, type TextGeometry } from '#lib/features/viewer/selection.ts';
import { FoundWords, type Suggestion } from '#lib/features/viewer/suggestions.ts';
import { clampZoom, type ZoomMode } from '#lib/features/viewer/zoom.ts';

import { PageMove } from './pagemap.ts';

/** What the viewer component exposes to menus, shortcuts and the view bar. */
export interface ViewerApi {
	position(): ViewPosition;
	goTo(position: ViewPosition, options?: { recordHistory?: boolean }): void;
	/** Scrolls so that `rect` (page points, view space) is visible; with `smooth`, gliding
	 * there (unless the system asks for reduced motion). */
	reveal(page: number, rect: [number, number, number, number], options?: { smooth?: boolean }): void;
	/**
	 * The page and point (page points, view space) at the top-left of what is visible:
	 * where a bookmark made now should lead.
	 */
	topLeft(): { page: number; x: number; y: number };
	/**
	 * Scrolls so that a point of a page (page points; null keeps the current position on
	 * that axis, or the page's top edge) is at the top-left of the view. Recorded in
	 * back/forward history. With `smooth`, the view glides there (unless the system asks
	 * for reduced motion).
	 */
	goToPoint(page: number, x: number | null, y: number | null, options?: { smooth?: boolean }): void;
	// Zooming glides there unless Settings or the system's reduced motion say otherwise.
	/** Zooms keeping the point at the center of the viewport in place. */
	setZoom(zoom: number, mode: ZoomMode): void;
	/** Zooms in (1) or out (-1) to the next preset, around the center of the viewport. */
	zoomStep(direction: 1 | -1): void;
	fit(mode: 'fitWidth' | 'fitPage'): void;
	/** Lays the pages out another way (section 6.1), keeping the current place. */
	setMode(mode: ViewMode, cover: boolean): void;
	/** Turns pages by `degrees` as they are shown (section 6.1, view rotation), keeping the
	 * current place. */
	turnPages(pages: readonly number[], degrees: number): void;
	/** Goes to the top of the previous or next page, or two pages in a two-page mode,
	 * gliding there unless Settings or the system's reduced motion say otherwise. */
	turn(direction: 1 | -1): void;
	focus(): void;
	/** Where the view is, in pages from the start (the part of a page as a fraction): what
	 * synchronized scrolling keeps panes in step by (section 6.1, split view). */
	place(): number;
	/** Goes to `place` (as `place()` gives it) to keep in step with a pane being scrolled,
	 * at once, outside back/forward history; this scroll leads no other pane. */
	follow(place: number): void;
}

export type BannerKind = 'changedOnDisk' | 'changedOnDiskDirty' | 'deletedOnDisk';

/** Something being drawn or dragged on a page, shown before Rust makes it (page points). */
export type AnnotationDraft =
	| { kind: 'move'; page: number; id: number; box: Box }
	| { kind: 'ink'; page: number; points: number[] }
	| { kind: 'area'; page: number; box: Box }
	/** A text box being typed: new (`id` null) or an existing one being edited. `turn`: how
	 * far its text is turned from the page as the document has it (for a new one, as far
	 * as the page is turned on screen, where it is typed upright). */
	| {
			kind: 'text';
			page: number;
			id: number | null;
			box: Box;
			turn: Rotation;
			text: string;
			fontSize: number;
			color: string;
	  };

export interface AnnotationRef {
	page: number;
	id: number;
}

export interface SearchHit {
	page: number;
	/** Quads, 8 numbers each (ul, ur, ll, lr), page points. */
	quads: number[][];
}

const TEXT_CACHE_PAGES = 120;
const SEARCH_CHUNK = 32;

/** Where a search starts: the first match at or after this page, or, with a point (page
 * points, view space at zoom 1), the match on the page that holds it, if one does. */
export interface SearchFrom {
	page: number;
	point?: [number, number];
}

/** How often the suggestions are worked out again while a search goes on, in ms. */
const SUGGEST_EVERY_MS = 200;

class SearchState {
	open = $state(false);
	/** The text in the search field, which a search starts from once typing pauses. */
	input = $state('');
	query = $state('');
	hits = $state<SearchHit[]>([]);
	current = $state(-1);
	running = $state(false);
	/** The search stopped with an error (the document closed or could not be read). */
	failed = $state(false);
	/** Pages searched so far, for progress. */
	searched = $state(0);
	/** The words of the document that contain the query, for the field's list. */
	suggestions = $state<Suggestion[]>([]);
	private readonly words = new FoundWords();
	private run = 0;
	/** A hit to go to as soon as the search has found it (Enter, Next or Previous pressed
	 * before it had). */
	private pending: { from: SearchFrom; direction: 1 | -1; reveal: boolean; smooth: boolean } | null = null;

	constructor(private readonly tab: DocTab) {}

	/** Hits grouped by page, for drawing. */
	byPage = $derived.by(() => {
		const map = new Map<number, { index: number; hit: SearchHit }[]>();
		this.hits.forEach((hit, index) => {
			const list = map.get(hit.page) ?? [];
			list.push({ index, hit });
			map.set(hit.page, list);
		});
		return map;
	});

	cancel() {
		this.run++;
		this.running = false;
	}

	clear() {
		this.cancel();
		this.pending = null;
		this.hits = [];
		this.current = -1;
		this.searched = 0;
		this.failed = false;
		this.words.clear();
		this.suggestions = [];
	}

	/**
	 * Searches the whole document in chunks. It moves nothing by itself (a search typed
	 * leaves the view where it is): with `go`, it then goes to the first hit at or after
	 * `from` ('reveal', gliding there with `smooth`), or makes that hit the one shown
	 * without moving the view ('mark').
	 */
	async start(query: string, from: number | SearchFrom, options: SearchGo = {}) {
		this.clear();
		this.query = query;
		if (!query.trim()) return;
		const run = ++this.run;
		this.running = true;
		const at = typeof from === 'number' ? { page: from } : from;
		if (options.go) this.pending = { from: at, direction: 1, reveal: options.go === 'reveal', smooth: options.smooth === true };
		const count = this.tab.pages.length;
		let suggested = 0;
		try {
			for (let start = 0; start < count; start += SEARCH_CHUNK) {
				const chunk = await searchText(this.tab.id, query, start, SEARCH_CHUNK);
				if (run !== this.run) return;
				const found = chunk.pages.flatMap((p) =>
					p.hits.map((flat) => ({ page: p.page, quads: splitQuads(flat) }))
				);
				this.hits = [...this.hits, ...found];
				this.searched = Math.min(count, start + SEARCH_CHUNK);
				this.words.add(chunk.words);
				if (performance.now() - suggested >= SUGGEST_EVERY_MS) {
					this.suggestions = this.words.suggest(query);
					suggested = performance.now();
				}
				this.settle(false);
			}
			this.suggestions = this.words.suggest(query);
			this.settle(true);
		} catch (e) {
			if (run === this.run) {
				this.failed = true;
				void logError(`search failed: ${String(e)}`);
			}
		} finally {
			if (run === this.run) {
				this.running = false;
				this.pending = null;
			}
		}
	}

	/** The search again, after the document changed: the hit shown stays the one shown, as
	 * far as it can, and the view stays where it is. */
	refresh() {
		if (!this.query) return;
		const shown = this.hits[this.current];
		void this.start(this.query, shown?.page ?? this.tab.currentPage, { go: shown ? 'mark' : undefined });
	}

	/** As the bar opens again with text in its field: that text is searched again, if it is
	 * not still. */
	resume() {
		if (!this.input.trim() || (this.input === this.query && (this.hits.length > 0 || this.running))) return;
		void this.start(this.input, this.tab.currentPage);
	}

	/** Goes to the hit waited for, once the pages it can be on have been searched (`done`:
	 * all of them). */
	private settle(done: boolean) {
		const wait = this.pending;
		if (!wait || this.current >= 0) return;
		let index = -1;
		if (wait.direction > 0) {
			index = firstHit(this.hits, wait.from);
			// Nothing at or after it: wrap to the first hit.
			if (index < 0 && done) index = this.hits.length ? 0 : -1;
		} else if (done || this.searched >= wait.from.page) {
			index = this.hits.findLastIndex((h) => h.page < wait.from.page);
			// Nothing before it: wrap to the last hit.
			if (index < 0 && done) index = this.hits.length - 1;
		}
		if (index < 0) return;
		this.pending = null;
		this.select(index, { smooth: wait.smooth, reveal: wait.reveal });
	}

	select(index: number, options: { smooth?: boolean; reveal?: boolean } = {}) {
		const hit = this.hits[index];
		if (!hit) return;
		this.current = index;
		if (options.reveal === false) return;
		const xs = hit.quads.flatMap((q) => [q[0]!, q[2]!, q[4]!, q[6]!]);
		const ys = hit.quads.flatMap((q) => [q[1]!, q[3]!, q[5]!, q[7]!]);
		this.tab.viewer?.reveal(
			hit.page,
			[Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)],
			{ smooth: options.smooth === true }
		);
	}

	/**
	 * The next or previous hit. With none shown yet (the search was typed, which moves
	 * nothing), the first from the current page on, or the last before it: once those pages
	 * have been searched, if the search is still going.
	 */
	step(direction: 1 | -1, options: { smooth?: boolean } = {}) {
		const n = this.hits.length;
		if (this.current >= 0) {
			this.select((((this.current + direction) % n) + n) % n, options);
			return;
		}
		if (!n && !this.running) return;
		this.pending = { from: { page: this.tab.currentPage }, direction, reveal: true, smooth: options.smooth === true };
		this.settle(!this.running);
	}
}

/** What a search does once it finds hits (`SearchState.start`). */
export interface SearchGo {
	go?: 'reveal' | 'mark';
	smooth?: boolean;
}

/** The hit a search starting at `from` shows first: -1 while none has been found. Only a
 * hit found already counts, so `hits` must hold every page up to `from.page`'s. With a
 * point, the hit whose first line holds it comes first. */
export function firstHit(hits: readonly SearchHit[], from: SearchFrom): number {
	const point = from.point;
	if (point) {
		const at = hits.findIndex((hit) => {
			const q = hit.quads[0];
			if (hit.page !== from.page || !q) return false;
			const xs = [q[0]!, q[2]!, q[4]!, q[6]!];
			const ys = [q[1]!, q[3]!, q[5]!, q[7]!];
			return (
				point[0] >= Math.min(...xs) - 1 &&
				point[0] <= Math.max(...xs) + 1 &&
				point[1] >= Math.min(...ys) - 1 &&
				point[1] <= Math.max(...ys) + 1
			);
		});
		if (at >= 0) return at;
	}
	return hits.findIndex((h) => h.page >= from.page);
}

/** The turns remembered for a file's pages (none in a view stored before pages could be
 * turned), for the pages it has now. */
function storedTurns(stored: ViewState['pageTurns'] | undefined, pageCount: number): Map<number, Rotation> {
	const turns = new Map<number, Rotation>();
	for (const [page, degrees] of Object.entries(stored ?? {})) {
		const turn = normalizeRotation(degrees);
		if (turn !== 0 && Number(page) < pageCount) turns.set(Number(page), turn);
	}
	return turns;
}

function splitQuads(flat: number[]): number[][] {
	const quads: number[][] = [];
	for (let i = 0; i + 8 <= flat.length; i += 8) quads.push(flat.slice(i, i + 8));
	return quads;
}

/** One open document, as Rust reported it: shared by every view of it in this window (a
 * document can be shown in two panes side by side, section 6.1, split view). */
export class DocModel {
	readonly id: number;
	name = $state('');
	path = $state('');
	/** Not saved under a name yet (pages extracted into a new document, section 6.7): it
	 * has unsaved changes until Save, which asks where to save it. */
	untitled = $state(false);
	pages = $state<PageSize[]>([]);
	/** One label per page as stored, or null when the document has no labels. */
	labels = $state<string[] | null>(null);
	/** The label rules exactly as stored (empty without labels). */
	labelRules = $state<LabelRule[]>([]);
	/** The labels the Page labels dialog would give, before it is applied (live preview,
	 * section 6.3). */
	previewLabels = $state<string[] | null>(null);
	/** What thumbnails, the page box and the page position show. */
	displayLabels = $derived(this.previewLabels ?? this.labels);
	/** What Rust reports about the file: what it allows and how it is saved. */
	fileFlags = $state<DocumentFlags>({
		encrypted: false,
		signed: false,
		repaired: false,
		canAssemble: true,
		canAnnotate: true,
		canCopy: true,
		pdfa: null
	});
	/**
	 * Open for reading only, as a file that claims PDF/A opens (section 5.4), until the user
	 * enables editing: Rust says so, and refuses every change meanwhile.
	 */
	readOnly = $state(false);
	/** What the document allows now: the file's flags, and no change of any kind while it
	 * is open for reading only. */
	flags = $derived<DocumentFlags>(
		this.readOnly ? { ...this.fileFlags, canAssemble: false, canAnnotate: false } : this.fileFlags
	);
	state = $state<DocumentState>({ revision: 0, dirty: false, undoName: null, redoName: null });
	outline = $state<Outline>({ items: [], damaged: false });
	/** The bookmark selected in the panel. */
	selectedBookmark = $state<number | null>(null);
	/** The bookmark whose title is being edited in the panel. */
	renamingBookmark = $state<number | null>(null);
	/** Annotations by page (pages without any are left out). */
	annotations = $state.raw<ReadonlyMap<number, Annotation[]>>(new Map());
	/** Every annotation, in page order. */
	allAnnotations = $derived.by(() => {
		const pages = [...this.annotations.keys()].sort((a, b) => a - b);
		return pages.flatMap((p) => this.annotations.get(p) ?? []);
	});
	/** A key for each page, in page order: a page keeps its key when pages are moved,
	 * deleted or inserted, so what is shown or selected by key follows its page. */
	pageKeys = $state.raw<number[]>([]);
	private nextPageKey = 1;
	banner = $state<BannerKind | null>(null);
	saving = $state(false);
	/** The signed-document warning was accepted for this session. */
	signedWarningAccepted = false;
	/** Bumps when cached text arrives, so overlays re-read the cache. */
	textVersion = $state(0);
	private texts = new Map<number, TextGeometry>();
	private textRequests = new Map<number, Promise<TextGeometry | null>>();
	/** The views showing it in this window. */
	readonly views = new Set<DocTab>();

	constructor(info: DocumentInfo) {
		this.id = info.id;
	}

	/** Replaces everything Rust reported (open, reload), in every view. */
	update(info: DocumentInfo) {
		this.name = info.name;
		this.path = info.path;
		this.untitled = info.untitled;
		this.pages = info.pages;
		// The file was read anew: nothing says which page is which of the ones before.
		this.pageKeys = info.pages.map(() => this.nextPageKey++);
		this.labels = info.labels;
		this.labelRules = info.labelRules;
		this.previewLabels = null;
		this.fileFlags = info.flags;
		this.readOnly = info.readOnly;
		this.state = info.state;
		this.setOutline(info.outline);
		this.setAnnotations(info.annotations, true);
		this.texts.clear();
		this.textRequests.clear();
		for (const view of this.views) view.readAgain();
	}

	/** Applies what a mutating command changed (section 3: no full re-fetch), in every view. */
	applyChange(change: DocumentChange) {
		const revisionChanged = change.state.revision !== this.state.revision;
		// Pages moved, deleted or inserted: where each view is, read before they change under it.
		const move = change.pageMap ? new PageMove(change.pageMap) : null;
		const shown = new Map([...this.views].map((view) => [view, move ? (view.viewer?.position() ?? null) : null]));
		this.state = change.state;
		const counted = change.changedPages.length > 0 || change.pageCount !== this.pages.length;
		if (counted) {
			// Inserted or deleted pages (or either undone) change the count; the pages that
			// moved are listed with their sizes.
			const pages = this.pages.slice(0, change.pageCount);
			for (const p of change.changedPages) pages[p.index] = p.size;
			this.pages = pages;
		}
		if (move) this.pageKeys = move.keys(this.pageKeys, () => this.nextPageKey++);
		for (const view of this.views) {
			if (counted && !move) view.currentPage = Math.min(view.currentPage, Math.max(0, change.pageCount - 1));
			if (move) view.pagesMoved(move, shown.get(view) ?? null);
		}
		if (change.labelsChanged) {
			this.labels = change.labels;
			this.labelRules = change.labelRules;
			this.previewLabels = null;
		}
		if (change.outline) this.setOutline(change.outline);
		if (change.annotations.length || change.pageCount !== this.pages.length) {
			this.setAnnotations(change.annotations, false, change.pageCount);
		}
		if (revisionChanged) for (const view of this.views) view.revised();
	}

	get pageCount() {
		return this.pages.length;
	}

	/** What a save changed beyond the state: ids, after Save As (optimized) renumbered objects. */
	applySaved(result: SaveResult) {
		if (result.outline) this.setOutline(result.outline);
		// A save that may have broken PDF/A took the claim out of the file (section 5.4).
		if (result.pdfaRemoved) {
			this.fileFlags = { ...this.fileFlags, pdfa: null };
			this.readOnly = false;
		}
		if (result.annotations.length) this.setAnnotations(result.annotations, false);
	}

	annotationsOn(page: number): readonly Annotation[] {
		return this.annotations.get(page) ?? [];
	}

	annotation(page: number, id: number): Annotation | null {
		return this.annotations.get(page)?.find((a) => a.id === id) ?? null;
	}

	/** Replies to an annotation, shown in its thread (ADR 0012). */
	repliesTo(page: number, id: number): Annotation[] {
		return this.annotationsOn(page).filter((a) => a.replyTo === id && id !== 0);
	}

	/** Is `a` a reply to an annotation on its page (shown in that one's thread)? */
	isThreadReply(a: Annotation): boolean {
		return a.replyTo !== null && this.annotation(a.page, a.replyTo) !== null;
	}

	/** The annotations on a page the pointer can reach: replies in a thread are not drawn
	 * on the page (ADR 0012). */
	hitTargets(page: number): Annotation[] {
		return this.annotationsOn(page).filter((a) => !this.isThreadReply(a));
	}

	/**
	 * Takes Rust's annotation lists: all of them (`replace`), or the pages that changed (an
	 * empty list removes a page). Pages past `pageCount` are dropped. Comments get LF line
	 * breaks (see `lineBreaks`). A view's selected annotation that is gone is deselected.
	 */
	private setAnnotations(pages: PageAnnotations[], replace: boolean, pageCount = this.pages.length) {
		const map = replace ? new Map<number, Annotation[]>() : new Map(this.annotations);
		for (const p of pages) {
			if (p.annotations.length) map.set(p.page, p.annotations.map((a) => ({ ...a, contents: lineBreaks(a.contents) })));
			else map.delete(p.page);
		}
		for (const page of map.keys()) if (page >= pageCount) map.delete(page);
		this.annotations = map;
		for (const view of this.views) {
			const sel = view.selectedAnnotation;
			if (sel && !map.get(sel.page)?.some((a) => a.id === sel.id)) view.selectedAnnotation = null;
		}
	}

	/** Page labels can be changed (permissions allow document changes). */
	get canEditLabels() {
		return this.flags.canAssemble;
	}

	/** Bookmarks can be changed: the outline is intact and permissions allow it. */
	get canEditBookmarks() {
		return !this.outline.damaged && this.flags.canAssemble;
	}

	private setOutline(outline: Outline) {
		this.outline = outline;
		const ids = new Set<number>();
		const walk = (items: Outline['items']) => {
			for (const b of items) {
				ids.add(b.id);
				walk(b.children);
			}
		};
		walk(outline.items);
		if (this.selectedBookmark !== null && !ids.has(this.selectedBookmark)) this.selectedBookmark = null;
		if (this.renamingBookmark !== null && !ids.has(this.renamingBookmark)) this.renamingBookmark = null;
	}

	/** Cached text geometry for the current revision, if loaded. */
	text(page: number): TextGeometry | undefined {
		const t = this.texts.get(page);
		return t && t.revision === this.state.revision ? t : undefined;
	}

	/** Loads a page's text geometry once per revision. */
	loadText(page: number): Promise<TextGeometry | null> {
		const cached = this.text(page);
		if (cached) return Promise.resolve(cached);
		const pending = this.textRequests.get(page);
		if (pending) return pending;
		const request = getPageText(this.id, page)
			.then((raw) => {
				const text = prepareText(raw);
				if (this.texts.size >= TEXT_CACHE_PAGES) {
					const oldest = this.texts.keys().next().value;
					if (oldest !== undefined) this.texts.delete(oldest);
				}
				this.texts.set(page, text);
				this.textVersion++;
				return text;
			})
			.catch(() => null)
			.finally(() => this.textRequests.delete(page));
		this.textRequests.set(page, request);
		return request;
	}

	/** Every cached text page, for building copied text. */
	textMap(): ReadonlyMap<number, TextGeometry> {
		return this.texts;
	}
}

let viewSeq = 0;

/**
 * One view of an open document: a tab, or a pane of tabs joined side by side (section 6.1,
 * split view). What Rust told us about the document is its `doc`, shared with the other
 * views of it; how the user is viewing it (zoom, place, selection, search) is its own.
 */
export class DocTab {
	readonly doc: DocModel;
	/** Tells this view from another of the same document. */
	readonly key = ++viewSeq;

	get id() {
		return this.doc.id;
	}
	get name() {
		return this.doc.name;
	}
	set name(value: string) {
		this.doc.name = value;
	}
	get path() {
		return this.doc.path;
	}
	set path(value: string) {
		this.doc.path = value;
	}
	get untitled() {
		return this.doc.untitled;
	}
	set untitled(value: boolean) {
		this.doc.untitled = value;
	}
	get pages() {
		return this.doc.pages;
	}
	set pages(value: PageSize[]) {
		this.doc.pages = value;
	}
	get labels() {
		return this.doc.labels;
	}
	set labels(value: string[] | null) {
		this.doc.labels = value;
	}
	get labelRules() {
		return this.doc.labelRules;
	}
	set labelRules(value: LabelRule[]) {
		this.doc.labelRules = value;
	}
	get previewLabels() {
		return this.doc.previewLabels;
	}
	set previewLabels(value: string[] | null) {
		this.doc.previewLabels = value;
	}
	get displayLabels() {
		return this.doc.displayLabels;
	}
	get fileFlags() {
		return this.doc.fileFlags;
	}
	set fileFlags(value: DocumentFlags) {
		this.doc.fileFlags = value;
	}
	get readOnly() {
		return this.doc.readOnly;
	}
	set readOnly(value: boolean) {
		this.doc.readOnly = value;
	}
	get flags() {
		return this.doc.flags;
	}
	get state() {
		return this.doc.state;
	}
	set state(value: DocumentState) {
		this.doc.state = value;
	}
	get outline() {
		return this.doc.outline;
	}
	get selectedBookmark() {
		return this.doc.selectedBookmark;
	}
	set selectedBookmark(value: number | null) {
		this.doc.selectedBookmark = value;
	}
	get renamingBookmark() {
		return this.doc.renamingBookmark;
	}
	set renamingBookmark(value: number | null) {
		this.doc.renamingBookmark = value;
	}
	get annotations() {
		return this.doc.annotations;
	}
	get allAnnotations() {
		return this.doc.allAnnotations;
	}
	get pageKeys() {
		return this.doc.pageKeys;
	}
	get banner() {
		return this.doc.banner;
	}
	set banner(value: BannerKind | null) {
		this.doc.banner = value;
	}
	get saving() {
		return this.doc.saving;
	}
	set saving(value: boolean) {
		this.doc.saving = value;
	}
	get signedWarningAccepted() {
		return this.doc.signedWarningAccepted;
	}
	set signedWarningAccepted(value: boolean) {
		this.doc.signedWarningAccepted = value;
	}
	get textVersion() {
		return this.doc.textVersion;
	}
	get pageCount() {
		return this.doc.pageCount;
	}
	get canEditLabels() {
		return this.doc.canEditLabels;
	}
	get canEditBookmarks() {
		return this.doc.canEditBookmarks;
	}

	selectedAnnotation = $state<AnnotationRef | null>(null);
	/** Something being drawn or dragged on a page. */
	draft = $state<AnnotationDraft | null>(null);
	/** The selected annotation's data. */
	selectedAnnotationInfo = $derived.by(() => {
		const sel = this.selectedAnnotation;
		return sel ? (this.doc.annotations.get(sel.page)?.find((a) => a.id === sel.id) ?? null) : null;
	});

	zoom = $state(1);
	zoomMode = $state<ZoomMode>('fitWidth');
	rotation = $state<Rotation>(0);
	/** The pages turned as they are shown, on top of `rotation` (section 6.1, view rotation):
	 * a page's index and its turn. On screen only, never in the file; remembered per file
	 * with the rest of the view. */
	pageTurns = $state.raw<ReadonlyMap<number, Rotation>>(new Map());
	/** The visual adjustments this document has of its own (section 6.1), or null while it
	 * has the ones Settings give every document. On screen only, never in the file;
	 * remembered per file with the rest of the view. */
	ownAdjustments = $state.raw<PageAdjustments | null>(null);
	/** How the pages are shown: the document's own adjustments, else those of Settings. */
	adjustments = $derived(this.ownAdjustments ?? pageDefaults.value);
	/** The CSS filter that shows a page so, or null for the pages as they are. */
	pageFilter = $derived(filterOf(this.adjustments));
	/** How much of the sepia paper tone lies over a page (0 to 1), or null for none. */
	pageTone = $derived(toneOf(this.adjustments));
	/** How the pages are laid out (section 6.1, scroll modes); not stored in the file. */
	mode = $state<ViewMode>('singlePageContinuous');
	/** In the two-page modes, the first page has a row of its own (a book's cover). */
	cover = $state(false);
	columns = $derived(columnsOf(this.mode));
	continuous = $derived(isContinuous(this.mode));
	currentPage = $state(0);
	/** Position to show when the viewer mounts (restored view, or the tab's last place). */
	pendingPosition: ViewPosition | null = null;
	/** The pages are being organized (section 6.7): a grid of them is on screen in place of
	 * the viewer. Null while the document is read. */
	organize = $state<OrganizeState | null>(null);

	selection = $state<{ anchor: Caret; focus: Caret } | null>(null);
	/** The text cursor a click with the Select tool or a markup tool put in the text (section
	 * 6.1), while nothing is selected: Shift with the arrow keys selects from there. */
	caret = $state<Caret | null>(null);

	readonly history = new NavHistory();
	readonly search = new SearchState(this);
	viewer: ViewerApi | null = null;

	/** `scrollMode` is the Settings choice for documents being opened. A tab moved here from
	 * another window brings what it had there instead (`handoff`). Given a view in place of
	 * the document's information, a second view of that document, as it is shown there. */
	constructor(
		info: DocumentInfo | DocTab,
		scrollMode: ScrollMode = 'singlePageContinuous',
		handoff: TabHandoff | null = null
	) {
		if (info instanceof DocTab) {
			this.doc = info.doc;
			this.doc.views.add(this);
			this.copyView(info);
			return;
		}
		this.doc = new DocModel(info);
		this.doc.views.add(this);
		this.doc.update(info);
		const asked = documentMode(info.pageLayout);
		if (handoff && handoff.scrollMode !== 'document') {
			this.mode = handoff.scrollMode;
			this.cover = handoff.cover;
			this.signedWarningAccepted = handoff.signedWarningAccepted;
		} else if (scrollMode !== 'document') this.mode = scrollMode;
		else if (asked) {
			this.mode = asked.mode;
			this.cover = asked.cover;
		}
		const view = info.view;
		if (view) {
			this.zoom = clampZoom(view.zoom);
			this.zoomMode = view.zoomMode;
			this.rotation = normalizeRotation(view.rotation);
			this.pageTurns = storedTurns(view.pageTurns, info.pages.length);
			this.ownAdjustments = view.ownAdjustments ? normalizeAdjustments(view.ownAdjustments) : null;
			const page = Math.min(view.page, Math.max(0, info.pages.length - 1));
			this.currentPage = page;
			this.pendingPosition = { page, offset: view.offset };
		}
	}

	/** A second view of `view`'s document, shown as `view` shows it. */
	static beside(view: DocTab): DocTab {
		return new DocTab(view);
	}

	private copyView(from: DocTab) {
		this.zoom = from.zoom;
		this.zoomMode = from.zoomMode;
		this.rotation = from.rotation;
		this.pageTurns = from.pageTurns;
		this.ownAdjustments = from.ownAdjustments;
		this.mode = from.mode;
		this.cover = from.cover;
		this.currentPage = from.currentPage;
		this.pendingPosition = from.viewer?.position() ?? from.pendingPosition ?? { page: from.currentPage, offset: 0 };
	}

	/** This view goes (its pane closed): the document no longer counts it. */
	dispose() {
		this.search.cancel();
		this.doc.views.delete(this);
	}

	/** Replaces everything Rust reported (open, reload). */
	update(info: DocumentInfo) {
		this.doc.update(info);
	}

	/** The document was read anew (`DocModel.update`): what this view keeps by page. */
	readAgain() {
		this.draft = null;
		this.selection = null;
		this.caret = null;
		this.currentPage = Math.min(this.currentPage, Math.max(0, this.pages.length - 1));
		// Turns are kept by page: one past the end of the file read anew has no page.
		if ([...this.pageTurns.keys()].some((page) => page >= this.pages.length)) {
			this.pageTurns = new Map([...this.pageTurns].filter(([page]) => page < this.pages.length));
		}
		this.organize?.pagesChanged();
	}

	/** The document has a new revision: the text selected and the search's hits are of the
	 * one before. */
	revised() {
		this.selection = null;
		this.caret = null;
		this.search.refresh();
	}

	/** How far page `index` is turned on screen: the view rotation and the page's own turn. */
	shownRotation(index: number): Rotation {
		return normalizeRotation(this.rotation + (this.pageTurns.get(index) ?? 0));
	}

	/** Turns pages by `degrees` as they are shown. The document stays as it is: nothing to
	 * save or undo. */
	turnPages(pages: readonly number[], degrees: number) {
		const turns = new Map(this.pageTurns);
		for (const page of pages) {
			if (page < 0 || page >= this.pages.length) continue;
			const turn = normalizeRotation((turns.get(page) ?? 0) + degrees);
			if (turn === 0) turns.delete(page);
			else turns.set(page, turn);
		}
		this.pageTurns = turns;
	}

	/** Applies what a mutating command changed (section 3: no full re-fetch), in every view
	 * of the document. */
	applyChange(change: DocumentChange) {
		this.doc.applyChange(change);
	}

	/** Everything this view keeps by page index follows its page: the current page, the
	 * view (`shown`: where the viewer was before the change), the pages turned on screen,
	 * the history of jumps and the selected annotation. */
	pagesMoved(move: PageMove, shown: ViewPosition | null) {
		this.currentPage = move.follow(this.currentPage);
		if (this.pageTurns.size) {
			const turns = new Map<number, Rotation>();
			for (const [before, turn] of this.pageTurns) {
				const page = move.to(before);
				if (page !== null) turns.set(page, turn);
			}
			this.pageTurns = turns;
		}
		this.history.remap((page) => move.to(page));
		const selected = this.selectedAnnotation;
		if (selected) {
			const page = move.to(selected.page);
			this.selectedAnnotation = page === null ? null : { ...selected, page };
		}
		if (this.pendingPosition) this.pendingPosition = move.position(this.pendingPosition);
		if (shown && this.viewer) {
			const next = move.position(shown);
			if (next.page !== shown.page) this.viewer.goTo(next, { recordHistory: false });
		}
		this.organize?.pagesChanged();
	}

	/** What a save changed beyond the state: ids, after Save As (optimized) renumbered objects. */
	applySaved(result: SaveResult) {
		this.doc.applySaved(result);
	}

	annotationsOn(page: number): readonly Annotation[] {
		return this.doc.annotationsOn(page);
	}

	annotation(page: number, id: number): Annotation | null {
		return this.doc.annotation(page, id);
	}

	/** Replies to an annotation, shown in its thread (ADR 0012). */
	repliesTo(page: number, id: number): Annotation[] {
		return this.doc.repliesTo(page, id);
	}

	/** Is `a` a reply to an annotation on its page (shown in that one's thread)? */
	isThreadReply(a: Annotation): boolean {
		return this.doc.isThreadReply(a);
	}

	/** The annotations on a page the pointer can reach: replies in a thread are not drawn
	 * on the page (ADR 0012). */
	hitTargets(page: number): Annotation[] {
		return this.doc.hitTargets(page);
	}

	selectAnnotation(page: number, id: number) {
		this.selectedAnnotation = { page, id };
		this.selection = null;
		this.caret = null;
	}

	/** Cached text geometry for the current revision, if loaded. */
	text(page: number): TextGeometry | undefined {
		return this.doc.text(page);
	}

	/** Loads a page's text geometry once per revision. */
	loadText(page: number): Promise<TextGeometry | null> {
		return this.doc.loadText(page);
	}

	/** Every cached text page, for building copied text. */
	textMap(): ReadonlyMap<number, TextGeometry> {
		return this.doc.textMap();
	}

	/** What the tab takes along to another window, besides its place in the document (which
	 * goes as the remembered view). */
	handoff(): TabHandoff {
		return { scrollMode: this.mode, cover: this.cover, signedWarningAccepted: this.signedWarningAccepted };
	}

	/** Changes how this document's pages are shown: it has adjustments of its own from here
	 * on, unless the change brings it back to the ones Settings give every document. */
	adjust(change: Partial<PageAdjustments>) {
		const next = normalizeAdjustments({ ...this.adjustments, ...change });
		this.ownAdjustments = sameAdjustments(next, pageDefaults.value) ? null : next;
	}

	/** Back to the adjustments Settings give every document. */
	resetAdjustments() {
		this.ownAdjustments = null;
	}

	viewState(): ViewState {
		const position = this.viewer?.position() ?? this.pendingPosition ?? { page: this.currentPage, offset: 0 };
		return {
			page: position.page,
			offset: position.offset,
			zoom: this.zoom,
			zoomMode: this.zoomMode,
			rotation: this.rotation,
			ownAdjustments: this.ownAdjustments ? { ...this.ownAdjustments } : null,
			pageTurns: Object.fromEntries(this.pageTurns)
		};
	}
}
