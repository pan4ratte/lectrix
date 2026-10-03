// One open document (one tab): what Rust told us about it, and how the user is viewing it.
// The document itself lives in Rust (section 7); this is only a view of it.

import {
	getPageText,
	logError,
	searchText,
	type DocumentChange,
	type DocumentFlags,
	type DocumentInfo,
	type DocumentState,
	type LabelRule,
	type Outline,
	type PageSize,
	type ViewState
} from '#lib/ipc/index.ts';
import { NavHistory, type ViewPosition } from '#lib/features/viewer/history.ts';
import { normalizeRotation, type Rotation } from '#lib/features/viewer/layout.ts';
import { prepareText, type Caret, type TextGeometry } from '#lib/features/viewer/selection.ts';
import { clampZoom, type ZoomMode } from '#lib/features/viewer/zoom.ts';

/** What the viewer component exposes to menus, shortcuts and the status bar. */
export interface ViewerApi {
	position(): ViewPosition;
	goTo(position: ViewPosition, options?: { recordHistory?: boolean }): void;
	/** Scrolls so that `rect` (page points, view space) is visible. */
	reveal(page: number, rect: [number, number, number, number]): void;
	/**
	 * The page and point (page points, view space) at the top-left of what is visible:
	 * where a bookmark made now should lead.
	 */
	topLeft(): { page: number; x: number; y: number };
	/**
	 * Scrolls so that a point of a page (page points; null keeps the current position on
	 * that axis, or the page's top edge) is at the top-left of the view. Recorded in
	 * back/forward history.
	 */
	goToPoint(page: number, x: number | null, y: number | null): void;
	/** Zooms keeping the point at the center of the viewport in place. */
	setZoom(zoom: number, mode: ZoomMode): void;
	fit(mode: 'fitWidth' | 'fitPage'): void;
	focus(): void;
}

export type BannerKind = 'changedOnDisk' | 'changedOnDiskDirty' | 'deletedOnDisk';

export interface SearchHit {
	page: number;
	/** Quads, 8 numbers each (ul, ur, ll, lr), page points. */
	quads: number[][];
}

const TEXT_CACHE_PAGES = 120;
const SEARCH_CHUNK = 32;

class SearchState {
	open = $state(false);
	query = $state('');
	hits = $state<SearchHit[]>([]);
	current = $state(-1);
	running = $state(false);
	/** The search stopped with an error (the document closed or could not be read). */
	failed = $state(false);
	/** Pages searched so far, for progress. */
	searched = $state(0);
	private run = 0;

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
		this.hits = [];
		this.current = -1;
		this.searched = 0;
		this.failed = false;
	}

	/** Searches the whole document in chunks, jumping to the first hit at or after `from`. */
	async start(query: string, from: number) {
		this.clear();
		this.query = query;
		if (!query.trim()) return;
		const run = ++this.run;
		this.running = true;
		const count = this.tab.pages.length;
		try {
			for (let start = 0; start < count; start += SEARCH_CHUNK) {
				const chunk = await searchText(this.tab.id, query, start, SEARCH_CHUNK);
				if (run !== this.run) return;
				const found = chunk.pages.flatMap((p) =>
					p.hits.map((flat) => ({ page: p.page, quads: splitQuads(flat) }))
				);
				this.hits = [...this.hits, ...found];
				this.searched = Math.min(count, start + SEARCH_CHUNK);
				if (this.current < 0) {
					const first = this.hits.findIndex((h) => h.page >= from);
					if (first >= 0) this.select(first);
				}
			}
			// Nothing at or after `from`: wrap to the first hit.
			if (this.current < 0 && this.hits.length) this.select(0);
		} catch (e) {
			if (run === this.run) {
				this.failed = true;
				void logError(`search failed: ${String(e)}`);
			}
		} finally {
			if (run === this.run) this.running = false;
		}
	}

	select(index: number) {
		const hit = this.hits[index];
		if (!hit) return;
		this.current = index;
		const xs = hit.quads.flatMap((q) => [q[0]!, q[2]!, q[4]!, q[6]!]);
		const ys = hit.quads.flatMap((q) => [q[1]!, q[3]!, q[5]!, q[7]!]);
		this.tab.viewer?.reveal(hit.page, [
			Math.min(...xs),
			Math.min(...ys),
			Math.max(...xs),
			Math.max(...ys)
		]);
	}

	step(direction: 1 | -1) {
		if (!this.hits.length) return;
		const n = this.hits.length;
		this.select((((this.current < 0 ? 0 : this.current + direction) % n) + n) % n);
	}
}

function splitQuads(flat: number[]): number[][] {
	const quads: number[][] = [];
	for (let i = 0; i + 8 <= flat.length; i += 8) quads.push(flat.slice(i, i + 8));
	return quads;
}

export class DocTab {
	readonly id: number;
	name = $state('');
	path = $state('');
	pages = $state<PageSize[]>([]);
	/** One label per page as stored, or null when the document has no labels. */
	labels = $state<string[] | null>(null);
	/** The label rules exactly as stored (empty without labels). */
	labelRules = $state<LabelRule[]>([]);
	/** Labels of a rule being edited, before it is applied (live preview, section 6.3). */
	previewLabels = $state<string[] | null>(null);
	/** What thumbnails, the page box and the status bar show. */
	displayLabels = $derived(this.previewLabels ?? this.labels);
	/** Start page of the label rule selected in the Page labels panel. */
	selectedLabelRule = $state<number | null>(null);
	flags = $state<DocumentFlags>({
		encrypted: false,
		signed: false,
		repaired: false,
		canAssemble: true,
		canAnnotate: true,
		canCopy: true
	});
	state = $state<DocumentState>({ revision: 0, dirty: false, undoName: null, redoName: null });
	outline = $state<Outline>({ items: [], damaged: false });
	/** The bookmark selected in the panel. */
	selectedBookmark = $state<number | null>(null);
	/** The bookmark whose title is being edited in the panel. */
	renamingBookmark = $state<number | null>(null);

	zoom = $state(1);
	zoomMode = $state<ZoomMode>('fitWidth');
	rotation = $state<Rotation>(0);
	currentPage = $state(0);
	/** Position to show when the viewer mounts (restored view, or the tab's last place). */
	pendingPosition: ViewPosition | null = null;

	selection = $state<{ anchor: Caret; focus: Caret } | null>(null);
	banner = $state<BannerKind | null>(null);
	saving = $state(false);
	/** The signed-document warning was accepted for this session. */
	signedWarningAccepted = false;

	readonly history = new NavHistory();
	readonly search = new SearchState(this);
	viewer: ViewerApi | null = null;

	/** Bumps when cached text arrives, so overlays re-read the cache. */
	textVersion = $state(0);
	private texts = new Map<number, TextGeometry>();
	private textRequests = new Map<number, Promise<TextGeometry | null>>();

	constructor(info: DocumentInfo) {
		this.id = info.id;
		this.update(info);
		const view = info.view;
		if (view) {
			this.zoom = clampZoom(view.zoom);
			this.zoomMode = view.zoomMode;
			this.rotation = normalizeRotation(view.rotation);
			const page = Math.min(view.page, Math.max(0, info.pages.length - 1));
			this.currentPage = page;
			this.pendingPosition = { page, offset: view.offset };
		}
	}

	/** Replaces everything Rust reported (open, reload). */
	update(info: DocumentInfo) {
		this.name = info.name;
		this.path = info.path;
		this.pages = info.pages;
		this.labels = info.labels;
		this.labelRules = info.labelRules;
		this.previewLabels = null;
		this.flags = info.flags;
		this.state = info.state;
		this.setOutline(info.outline);
		this.texts.clear();
		this.textRequests.clear();
		this.selection = null;
		this.currentPage = Math.min(this.currentPage, Math.max(0, info.pages.length - 1));
	}

	/** Applies what a mutating command changed (section 3: no full re-fetch). */
	applyChange(change: DocumentChange) {
		const revisionChanged = change.state.revision !== this.state.revision;
		this.state = change.state;
		if (change.changedPages.length) {
			const pages = [...this.pages];
			for (const p of change.changedPages) pages[p.index] = p.size;
			this.pages = pages;
		}
		if (change.labelsChanged) {
			this.labels = change.labels;
			this.labelRules = change.labelRules;
			this.previewLabels = null;
		}
		if (change.outline) this.setOutline(change.outline);
		if (revisionChanged) {
			this.selection = null;
			if (this.search.query) void this.search.start(this.search.query, this.currentPage);
		}
	}

	get pageCount() {
		return this.pages.length;
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

	viewState(): ViewState {
		const position = this.viewer?.position() ?? this.pendingPosition ?? { page: this.currentPage, offset: 0 };
		return {
			page: position.page,
			offset: position.offset,
			zoom: this.zoom,
			zoomMode: this.zoomMode,
			rotation: this.rotation
		};
	}
}
