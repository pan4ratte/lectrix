<script lang="ts">
	// The page canvas: a virtualized, continuously scrolling column of pages. Only pages
	// within one screen of the viewport are mounted (section 3); the rest are space.
	import { ContextMenu } from 'bits-ui';
	import { onMount, tick, untrack } from 'svelte';

	import { chain } from '#lib/components/chain.ts';
	import { ClickCounter } from '#lib/components/clicks.ts';
	import { motionMs } from '#lib/components/panes.ts';
	import {
		commitTextDraft,
		copiesSelection,
		copyComment,
		copyInContext,
		create,
		createMarkup,
		hiddenByList,
		hiddenOnPage,
		listShowsSelected,
		madeForComment,
		markSelection,
		openComment,
		openInspector,
		openProperties,
		panelOpen,
		refuseIfLocked,
		remove,
		selectedKey,
		setPanel,
		update
	} from '#lib/features/annotations/actions.ts';
	import AnnotationBar from '#lib/features/annotations/AnnotationBar.svelte';
	import AnnotationInspector from '#lib/features/annotations/AnnotationInspector.svelte';
	import CommentTip from '#lib/features/annotations/CommentTip.svelte';
	import { DEFAULT_TIP_DELAY_MS, quickToolAllowed, releaseAnchor, type Area } from '#lib/features/annotations/bars.ts';
	import {
		annotationAt,
		boxQuad,
		clampBox,
		handleAt,
		moveBox,
		normalizeBox,
		resizeBox,
		turnBox,
		turnPoint,
		unturnBox,
		unturnPoint,
		type Box,
		type Handle
	} from '#lib/features/annotations/geometry.ts';
	import SelectionBar from '#lib/features/annotations/SelectionBar.svelte';
	import { tools } from '#lib/features/annotations/state.svelte.ts';
	import { DEFAULT_QUICK_TOOLS, capabilities, isMarkupTool, showsComment } from '#lib/features/annotations/tools.ts';
	import { scrolled } from '#lib/features/split/sync.ts';
	import { pageUrl, type Annotation, type PageLink } from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import type { ViewPosition } from './history.ts';
	import { selectedText } from './actions.ts';
	import { follow, linkAt, linkPreview } from './links.ts';
	import {
		CSS_PX_PER_PT,
		computeLayout,
		contentWidth,
		currentPage,
		isShown,
		normalizeRotation,
		pageAt,
		pageAtY,
		pageLeft,
		pagesBeside,
		pagesInRange,
		rowAtY,
		rowCount,
		rowOf,
		rowStart,
		turnedSize,
		type Arrangement,
		type ViewMode
	} from './layout.ts';
	import PageView from './PageView.svelte';
	import { AHEAD_PRIORITY, scheduler, type Ticket } from './scheduler.ts';
	import SearchBar from './SearchBar.svelte';
	import { CURRENT_MARK_PX, MARK_PX, searchMarks } from './searchmarks.ts';
	import {
		caretBox,
		caretPoint,
		compareCarets,
		edgeAt,
		extendByUnit,
		hitTest,
		isOverText,
		lineAt,
		moveCaret,
		ordered,
		selectionIsEmpty,
		selectionRects,
		wordAt,
		type Caret,
		type CaretStep,
		type SelectionEdge,
		type SelectionUnit
	} from './selection.ts';
	import {
		ZOOM_GLIDE_MS,
		clampZoom,
		easeOut,
		fitPageZoom,
		fitWidthZoom,
		isWheelNotch,
		renderScale,
		stepZoom,
		wheelZoomFactor,
		wholePageScale,
		zoomBetween,
		type ZoomMode
	} from './zoom.ts';

	let { tab }: { tab: DocTab } = $props();

	let scroller: HTMLDivElement | undefined = $state();
	let scrollTop = $state(0);
	let scrollLeft = $state(0);
	let viewportW = $state(0);
	let viewportH = $state(0);
	/** How wide the pages' vertical scrollbar is (0 without one). */
	let scrollbarW = $state(0);
	let dpr = $state(typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1);
	/** Where the pages' container begins in the window, in CSS pixels: a fraction of a
	 * device pixel under a bar of fractional height, which the pages make up for. */
	let originX = $state(0);
	let originY = $state(0);

	/** The row shown in the single page and two-page modes (section 6.1, scroll modes). */
	let shownRow = $state(untrack(() => rowOf(tab.currentPage, tab.columns, tab.cover)));
	const arrangement: Arrangement = $derived({
		columns: tab.columns,
		cover: tab.cover,
		row: tab.continuous ? null : shownRow
	});
	/** How far a page is turned on screen: the view rotation and the page's own turn. */
	const rotationOf = (index: number) => tab.shownRotation(index);
	const layout = $derived(computeLayout(tab.pages, tab.zoom, rotationOf, arrangement));
	const contentW = $derived(contentWidth(layout, viewportW));
	/** Pages mounted: the viewport plus a quarter screen above and below. Each mounted page
	 * holds its pixels in the webview, and a wider margin only cost memory (ADR 0002); the
	 * rows next to these are rendered ahead in Rust instead (below). */
	const mounted = $derived(pagesInRange(layout, scrollTop - viewportH * 0.25, scrollTop + viewportH * 1.25));
	const mountedPages = $derived.by(() => {
		// None before the viewport is measured: they would ask for images at the default zoom.
		if (viewportW === 0) return [];
		const [first, last] = mounted;
		const out: number[] = [];
		for (let i = first; i <= last; i++) out.push(i);
		return out;
	});

	$effect(() => {
		if (viewportH > 0) tab.currentPage = currentPage(layout, scrollTop, viewportH);
	});

	/** The marks of the search's matches beside the scrollbar (section 6.1), while the search
	 * bar is open and Settings have them. */
	const marks = $derived.by(() => {
		const search = tab.search;
		if (!search.open || app.settings?.searchMarkers === false || search.hits.length === 0) return null;
		return searchMarks(
			layout,
			search.hits,
			search.current,
			(i) => {
				const q = search.hits[i]!.quads[0];
				if (!q || !tab.pages[search.hits[i]!.page]) return null;
				const x = (q[0]! + q[2]! + q[4]! + q[6]!) / 4;
				const y = (q[1]! + q[3]! + q[5]! + q[7]!) / 4;
				return toBox(search.hits[i]!.page, x, y)[1];
			},
			viewportH
		);
	});

	function pageBox(index: number) {
		const box = layout.pages[index]!;
		return { ...box, left: pageLeft(layout, index, contentW) };
	}

	/** Visible part of a page (plus a margin for tiles), relative to the page box. */
	function visibleRect(index: number) {
		const b = pageBox(index);
		const margin = 256;
		const x0 = Math.max(0, scrollLeft - margin - b.left);
		const y0 = Math.max(0, scrollTop - margin - b.top);
		const x1 = Math.min(b.width, scrollLeft + viewportW + margin - b.left);
		const y1 = Math.min(b.height, scrollTop + viewportH + margin - b.top);
		return x1 > x0 && y1 > y0 ? { x0, y0, x1, y1 } : null;
	}

	function isOnscreen(index: number) {
		const b = pageBox(index);
		return b.top < scrollTop + viewportH && b.top + b.height > scrollTop;
	}

	/** Lower is sooner: visible pages by distance from the viewport's center, then the rest. */
	function priority(index: number) {
		const b = pageBox(index);
		const center = scrollTop + viewportH / 2;
		const distance = Math.abs(b.top + b.height / 2 - center);
		return (isOnscreen(index) ? 0 : 100_000) + distance;
	}

	function onScroll() {
		if (!scroller) return;
		scrollTop = scroller.scrollTop;
		scrollLeft = scroller.scrollLeft;
		closeMenuOffscreen();
		// Panes locked together (split view) follow this one, unless it is following them:
		// then this is its own move (`followPlace`), or the browser's event for it.
		if (following > 0) return;
		if (followedTop !== null && Math.abs(scroller.scrollTop - followedTop) < 1) {
			followedTop = null;
			return;
		}
		followedTop = null;
		scrolled(tab);
	}

	/** `followPlace` or `followZoom` is moving the view (how many of them): the scroll leads
	 * no other pane. */
	let following = 0;
	/** Where `followPlace` put the view, until the browser's scroll event for it arrives. */
	let followedTop: number | null = null;

	/** Pages from the start of the document at the top of the view (split view, section 6.1). */
	function place(): number {
		const p = position();
		return p.page + p.offset;
	}

	/** Goes to `at` (as `place` gives it) to keep in step with another pane. */
	async function followPlace(at: number) {
		if (layout.pages.length === 0) return;
		const page = Math.min(Math.max(0, Math.floor(at)), layout.pages.length - 1);
		const offset = Math.min(1, Math.max(0, at - page));
		stopScrollGlide();
		following++;
		try {
			const showing = showRowOf(page);
			if (showing) await showing;
			if (!scroller) return;
			const b = layout.pages[page];
			if (!b) return;
			const top = Math.round(b.top + offset * b.height);
			if (Math.abs(scroller.scrollTop - top) < 1) return;
			scroller.scrollTop = top;
			onScroll();
			followedTop = scroller.scrollTop;
		} finally {
			following--;
		}
	}

	// ----- positions -----

	/** Where the view is, from the scroll position last heard of: asked for as the viewer is
	 * taken down too (the tab keeps its place), when the scroller itself is gone already. */
	function position(): ViewPosition {
		if (layout.pages.length === 0) return { page: 0, offset: 0 };
		const page = Math.max(0, pageAtY(layout, scrollTop + 1));
		const b = layout.pages[page]!;
		return { page, offset: Math.min(1, Math.max(0, (scrollTop - b.top) / b.height)) };
	}

	/**
	 * In the single page and two-page modes, shows the row holding `page`: a promise that
	 * settles once the content has its new size; null when nothing changes.
	 */
	function showRowOf(page: number): Promise<void> | null {
		if (tab.continuous) return null;
		const row = Math.min(rowOf(Math.max(0, page), tab.columns, tab.cover), Math.max(0, layout.rows.length - 1));
		if (row === shownRow) return null;
		shownRow = row;
		return tick();
	}

	async function scrollToPosition(p: ViewPosition) {
		// A glide under way would go on to its own place over this one.
		stopScrollGlide();
		const showing = showRowOf(p.page);
		if (showing) await showing;
		if (!scroller) return;
		const page = Math.min(Math.max(0, p.page), layout.pages.length - 1);
		const b = layout.pages[page];
		if (!b) return;
		const offset = p.offset > 0 ? p.offset * b.height : -8;
		scroller.scrollTop = b.top + offset;
		onScroll();
	}

	function goTo(p: ViewPosition, options: { recordHistory?: boolean } = {}) {
		if (options.recordHistory !== false) tab.history.push(position());
		void scrollToPosition(p);
	}

	/** In the single page and two-page modes, shows row `row` from its top or its bottom. */
	async function showRow(row: number, end: 'top' | 'bottom') {
		if (row < 0 || row >= rowCount(tab.pages.length, tab.columns, tab.cover)) return;
		stopScrollGlide();
		const showing = showRowOf(rowStart(row, tab.columns, tab.cover));
		if (showing) await showing;
		if (!scroller) return;
		scroller.scrollTop = end === 'top' ? 0 : scroller.scrollHeight;
		onScroll();
	}

	/** Shows the row after the one shown, from its top, or the one before, from its bottom. */
	function flip(direction: 1 | -1) {
		return showRow(shownRow + direction, direction > 0 ? 'top' : 'bottom');
	}

	/** The row a turn is on its way to: a turn pressed meanwhile goes on from there. */
	let turningTo: number | null = null;

	/**
	 * Goes to the top of the page (or two-page row) before or after the current one: gliding
	 * there, or, with one row shown at a time, with that row coming in from the side it is
	 * on. At once if Settings or the system's reduced motion say so.
	 */
	function turn(direction: 1 | -1) {
		const row = (turningTo ?? rowOf(tab.currentPage, tab.columns, tab.cover)) + direction;
		if (row < 0 || row >= rowCount(tab.pages.length, tab.columns, tab.cover)) return;
		const page = rowStart(row, tab.columns, tab.cover);
		const duration = app.settings?.smoothPageTurn === false ? 0 : motionMs(SCROLL_GLIDE_MS);
		const box = layout.pages[page];
		if (tab.continuous && duration > 0 && box) {
			glideScroll(box.top - 8, null, duration);
			turningTo = row;
			return;
		}
		const single = !tab.continuous;
		const arrived = scrollToPosition({ page, offset: 0 });
		// Until the current page has followed the scroll, a turn pressed again goes on from
		// this row, as during a glide.
		turningTo = row;
		void arrived
			.then(() => {
				if (single) enterRow(direction, duration);
				return tick();
			})
			.then(() => {
				if (turningTo === row) turningTo = null;
			});
	}

	/** How far from its place a row starts when it comes in, in CSS pixels. */
	const ROW_ENTER_PX = 24;
	/** The pages' layer inside the scroller. */
	let content: HTMLDivElement | undefined = $state();

	/**
	 * In the single page and two-page modes, has the row just shown come in: rising into
	 * place as it fades in for a row after the one it replaces (`direction` 1), coming down
	 * for one before it. Transform and opacity alone, which lay nothing out.
	 */
	function enterRow(direction: 1 | -1, duration: number) {
		if (duration === 0) return;
		content?.animate(
			[
				{ opacity: 0, transform: `translateY(${direction * ROW_ENTER_PX}px)` },
				{ opacity: 1, transform: 'none' }
			],
			{ duration, easing: 'ease-out' }
		);
	}

	/** Lays the pages out anew, as `change` says, keeping the current page (the one the page
	 * box shows) and how far down it the view is, and fits the zoom again in a fit mode (two
	 * pages make a wider row, a page on its side a wider page). */
	async function layOut(change: () => void) {
		// Not `position()`: just after going to a page, the top of the view is in the gap
		// above it, and that names the page before.
		const page = tab.currentPage;
		const box = layout.pages[page];
		const at = { page, offset: box ? Math.min(1, Math.max(0, (scrollTop - box.top) / box.height)) : 0 };
		stopScrollGlide();
		if (stopZoomGlide()) heldRenderZoom = null;
		change();
		shownRow = rowOf(at.page, tab.columns, tab.cover);
		if (tab.zoomMode !== 'custom' && viewportW > 0) tab.zoom = fitZoom(tab.zoomMode, at.page);
		await tick();
		await scrollToPosition(at);
	}

	/** Lays the pages out in another scroll mode (section 6.1). */
	async function setMode(mode: ViewMode, cover: boolean) {
		if (mode === tab.mode && cover === tab.cover) return;
		await layOut(() => {
			tab.mode = mode;
			tab.cover = cover;
		});
	}

	/** Turns pages as they are shown (section 6.1, view rotation). Only how they are laid
	 * out changes: their pixels are the ones they have, turned with their layer. */
	function turnPages(pages: readonly number[], degrees: number) {
		void layOut(() => tab.turnPages(pages, degrees));
	}

	/** Maps a point in page points (unrotated) to CSS pixels in the rotated page box. */
	function toBox(index: number, x: number, y: number): [number, number] {
		const k = tab.zoom * CSS_PX_PER_PT;
		const s = tab.pages[index]!;
		const [bx, by] = turnPoint(x, y, rotationOf(index), s.width, s.height);
		return [bx * k, by * k];
	}

	/** Maps a client (screen) point to page points (unrotated) on page `index`. */
	function toPage(index: number, clientX: number, clientY: number): [number, number] {
		const rect = scroller!.getBoundingClientRect();
		const b = pageBox(index);
		return boxToPage(index, clientX - rect.left + scrollLeft - b.left, clientY - rect.top + scrollTop - b.top);
	}

	/** Maps CSS pixels in the rotated page box to page points (unrotated): the inverse of toBox. */
	function boxToPage(index: number, dx: number, dy: number): [number, number] {
		const k = tab.zoom * CSS_PX_PER_PT;
		const s = tab.pages[index]!;
		return unturnPoint(dx / k, dy / k, rotationOf(index), s.width, s.height);
	}

	function topLeft(): { page: number; x: number; y: number } {
		if (!scroller || layout.pages.length === 0) return { page: 0, x: 0, y: 0 };
		let row = Math.max(0, rowAtY(layout, scrollTop + 1));
		// In the gap below a row, the next row is the one coming into view.
		const below = layout.rows[row]!;
		if (scrollTop >= below.top + below.height - 1 && row < layout.shown[1]) row++;
		// Of two pages side by side, the first one in view.
		const { first, last } = layout.rows[row]!;
		let page = first;
		while (page < last && pageBox(page).left + pageBox(page).width <= scrollLeft) page++;
		const b = pageBox(page);
		const clampX = (v: number) => Math.min(b.width, Math.max(0, v));
		const clampY = (v: number) => Math.min(b.height, Math.max(0, v));
		const x0 = clampX(scrollLeft - b.left);
		const x1 = clampX(scrollLeft + viewportW - b.left);
		const y0 = clampY(scrollTop - b.top);
		const y1 = clampY(scrollTop + viewportH - b.top);
		// The visible part's top-left corner in the page's own orientation (the page may be
		// turned on screen).
		const corners = [boxToPage(page, x0, y0), boxToPage(page, x1, y0), boxToPage(page, x0, y1), boxToPage(page, x1, y1)];
		return {
			page,
			x: Math.min(...corners.map((c) => c[0])),
			y: Math.min(...corners.map((c) => c[1]))
		};
	}

	/** The part of page `index` that is in view, in page points (unrotated); null when none
	 * of it is. What the Pages panel outlines while the view is dragged there. */
	function visibleArea(index: number): [number, number, number, number] | null {
		if (!scroller || !layout.pages[index] || !isShown(layout, index)) return null;
		const b = pageBox(index);
		const x0 = Math.max(0, scrollLeft - b.left);
		const x1 = Math.min(b.width, scrollLeft + viewportW - b.left);
		const y0 = Math.max(0, scrollTop - b.top);
		const y1 = Math.min(b.height, scrollTop + viewportH - b.top);
		if (x1 <= x0 || y1 <= y0) return null;
		const corners = [boxToPage(index, x0, y0), boxToPage(index, x1, y0), boxToPage(index, x0, y1), boxToPage(index, x1, y1)];
		const xs = corners.map((c) => c[0]);
		const ys = corners.map((c) => c[1]);
		return [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)];
	}

	/** Scrolls so that a point of page `index` (page points, unrotated) is at the middle of
	 * the view, as far as the view goes, at once and outside back/forward history: the view
	 * dragged in the Pages panel. */
	async function centerOn(index: number, x: number, y: number) {
		const showing = showRowOf(index);
		if (showing) await showing;
		if (!scroller || !layout.pages[index]) return;
		stopScrollGlide();
		const b = pageBox(index);
		const [bx, by] = toBox(index, x, y);
		scroller.scrollTop = b.top + by - viewportH / 2;
		scroller.scrollLeft = b.left + bx - viewportW / 2;
		onScroll();
	}

	async function goToPoint(index: number, x: number | null, y: number | null, options: { smooth?: boolean } = {}) {
		if (!scroller) return;
		const page = Math.min(Math.max(0, index), layout.pages.length - 1);
		if (!layout.pages[page]) return;
		tab.history.push(position());
		const duration = options.smooth ? motionMs(SCROLL_GLIDE_MS) : 0;
		const fromRow = shownRow;
		const showing = showRowOf(page);
		if (showing) await showing;
		if (!scroller) return;
		const b = pageBox(page);
		const [bx, by] = toBox(page, x ?? 0, y ?? 0);
		// Place the point at the top-left corner of the page's content as it appears on
		// screen: on a page turned there, that corner is elsewhere in the box.
		let top: number;
		let left: number | null;
		switch (rotationOf(page)) {
			case 90:
				top = b.top + by;
				left = y === null ? null : b.left + bx - viewportW;
				break;
			case 180:
				top = b.top + by - viewportH;
				left = x === null ? null : b.left + bx - viewportW;
				break;
			case 270:
				top = b.top + by - viewportH;
				left = y === null ? null : b.left + bx;
				break;
			default:
				top = y === null ? b.top - 8 : b.top + by;
				left = x === null ? null : b.left + bx;
		}
		if (showing) {
			// Another row, with one shown at a time: there is no way to glide; it comes in.
			stopScrollGlide();
			scroller.scrollTop = top;
			if (left !== null) scroller.scrollLeft = left;
			onScroll();
			enterRow(shownRow > fromRow ? 1 : -1, duration);
			return;
		}
		// A long way starts one screen from the place, where its page is in view from the
		// first frame: from further off, pages passed on the way would be rendered first.
		glideScroll(top, left, duration, viewportH);
	}

	async function reveal(index: number, rect: [number, number, number, number], options: { smooth?: boolean } = {}) {
		const duration = options.smooth ? motionMs(SCROLL_GLIDE_MS) : 0;
		const fromRow = shownRow;
		const showing = showRowOf(index);
		if (showing) await showing;
		if (!scroller || !layout.pages[index]) return;
		const [ax, ay] = toBox(index, rect[0], rect[1]);
		const [bx, by] = toBox(index, rect[2], rect[3]);
		const b = pageBox(index);
		const top = b.top + Math.min(ay, by);
		const bottom = b.top + Math.max(ay, by);
		const left = b.left + Math.min(ax, bx);
		const right = b.left + Math.max(ax, bx);
		const toTop = top < scrollTop + 40 || bottom > scrollTop + viewportH - 40 ? (top + bottom) / 2 - viewportH / 2 : null;
		const toLeft = left < scrollLeft || right > scrollLeft + viewportW ? (left + right) / 2 - viewportW / 2 : null;
		if (showing) {
			// Another row, with one shown at a time: it comes in, as a row turned to does.
			glideScroll(toTop, toLeft, 0);
			enterRow(shownRow > fromRow ? 1 : -1, duration);
			return;
		}
		glideScroll(toTop, toLeft, duration);
	}

	// ----- gliding scroll (to an annotation picked in the list, section 6.5; to the next or
	// previous page and to a bookmark's place, section 6.1) -----

	const SCROLL_GLIDE_MS = 140;
	let scrollFrame = 0;

	/**
	 * Scrolls to `top` and `left` (null leaves that axis), over `duration` ms. A long way
	 * starts `near` from the target (two screens, unless given), so the pages in between
	 * are not all rendered.
	 */
	function glideScroll(top: number | null, left: number | null, duration: number, near = 2 * viewportH) {
		if (!scroller) return;
		stopScrollGlide();
		const maxTop = scroller.scrollHeight - scroller.clientHeight;
		const maxLeft = scroller.scrollWidth - scroller.clientWidth;
		const toTop = top === null ? null : Math.min(maxTop, Math.max(0, top));
		const toLeft = left === null ? null : Math.min(maxLeft, Math.max(0, left));
		if (toTop === null && toLeft === null) return;
		if (duration === 0) {
			if (toTop !== null) scroller.scrollTop = toTop;
			if (toLeft !== null) scroller.scrollLeft = toLeft;
			onScroll();
			return;
		}
		let fromTop = scroller.scrollTop;
		if (toTop !== null && Math.abs(toTop - fromTop) > near) fromTop = toTop - Math.sign(toTop - fromTop) * near;
		const fromLeft = scroller.scrollLeft;
		const start = performance.now();
		const frame = (now: number) => {
			if (!scroller) return;
			const e = easeOut((now - start) / duration);
			if (toTop !== null) scroller.scrollTop = fromTop + (toTop - fromTop) * e;
			if (toLeft !== null) scroller.scrollLeft = fromLeft + (toLeft - fromLeft) * e;
			onScroll();
			scrollFrame = e < 1 ? requestAnimationFrame(frame) : 0;
			if (e >= 1) turningTo = null;
		};
		scrollFrame = requestAnimationFrame(frame);
	}

	function stopScrollGlide() {
		cancelAnimationFrame(scrollFrame);
		scrollFrame = 0;
		turningTo = null;
	}

	// ----- zoom -----

	/**
	 * Zooms so that the content point under (ax, ay) in the viewport stays there. With
	 * `align`, the view then moves that far (0 to 1) toward the top of `align.page`.
	 */
	async function zoomAround(
		zoom: number,
		mode: ZoomMode,
		ax: number,
		ay: number,
		align?: { page: number; amount: number }
	) {
		if (!scroller || layout.pages.length === 0) return;
		const before = layout;
		const y = scrollTop + ay;
		const page = Math.max(0, pageAt(before, scrollLeft + ax, y, contentW));
		const b = before.pages[page]!;
		const fy = (y - b.top) / b.height;
		const left = pageLeft(before, page, contentW);
		const fx = (scrollLeft + ax - left) / b.width;

		// Where the view goes, worked out from the layout at the new zoom before the zoom
		// changes, so the zoom and the scroll position the mounted pages come from change
		// together. The new layout with the old scroll position names other pages: mounted
		// for the moment until the DOM caught up, they dropped the pages on screen and their
		// pixels, and each frame of a glide or a pinch flickered blank (tests/e2e zoom).
		const z = clampZoom(zoom);
		const next = computeLayout(tab.pages, z, rotationOf, arrangement);
		const nextW = contentWidth(next, viewportW);
		const after = next.pages[page]!;
		let toTop = after.top + fy * after.height - ay;
		const target = align && next.pages[align.page];
		// The same 8 px above the page as going to a page (scrollToPosition).
		if (align && target) toTop += (target.top - 8 - toTop) * align.amount;
		// Kept within the content, as the browser will.
		toTop = Math.min(Math.max(0, toTop), Math.max(0, next.totalHeight - viewportH));
		const toLeft = Math.min(
			Math.max(0, pageLeft(next, page, nextW) + fx * after.width - ax),
			Math.max(0, nextW - viewportW)
		);

		tab.zoomMode = mode;
		tab.zoom = z;
		scrollTop = toTop;
		scrollLeft = toLeft;
		// The content takes its new size in the DOM before it can scroll there.
		await tick();
		scroller.scrollTop = toTop;
		scroller.scrollLeft = toLeft;
		onScroll();
	}

	// Zooming glides to the new zoom (section 6.1): the + and - buttons, the zoom menu,
	// fitting and mouse wheel notches. Pages keep their pixels, stretched, until it
	// arrives, then render once. Settings turn it off, and so does reduced motion.
	let zoomGlide: { to: number; frame: number } | null = null;

	/**
	 * Glides to `to`, keeping the content under `anchor` (viewport pixels; the center if
	 * null) in place. With `alignPage`, the view ends at the top of that page.
	 */
	function glideZoom(to: number, mode: ZoomMode, anchor: { x: number; y: number } | null, alignPage: number | null = null) {
		const target = clampZoom(to);
		const at = anchor ?? { x: viewportW / 2, y: viewportH / 2 };
		const duration = app.settings?.smoothZoom === false ? 0 : motionMs(ZOOM_GLIDE_MS);
		if (stopZoomGlide() && duration === 0) heldRenderZoom = null;
		if (duration === 0) {
			const align = alignPage === null ? undefined : { page: alignPage, amount: 1 };
			void zoomAround(target, mode, at.x, at.y, align);
			return;
		}
		heldRenderZoom ??= tab.zoom;
		clearTimeout(settleTimer);
		const from = tab.zoom;
		const start = performance.now();
		const frame = (now: number) => {
			const t = (now - start) / duration;
			const align = alignPage === null ? undefined : { page: alignPage, amount: easeOut(t) };
			void zoomAround(zoomBetween(from, target, t), mode, at.x, at.y, align);
			if (t < 1) {
				zoomGlide = { to: target, frame: requestAnimationFrame(frame) };
			} else {
				zoomGlide = null;
				heldRenderZoom = null;
			}
		};
		zoomGlide = { to: target, frame: requestAnimationFrame(frame) };
	}

	/** Stops a glide where it is; true if one was under way. */
	function stopZoomGlide(): boolean {
		if (!zoomGlide) return false;
		cancelAnimationFrame(zoomGlide.frame);
		zoomGlide = null;
		return true;
	}

	/** Zooms at once around the center: re-fitting while the viewport changes size. */
	function setZoomNow(zoom: number, mode: ZoomMode) {
		if (stopZoomGlide()) heldRenderZoom = null;
		void zoomAround(zoom, mode, viewportW / 2, viewportH / 2);
	}

	function setZoom(zoom: number, mode: ZoomMode) {
		glideZoom(zoom, mode, null);
	}

	/**
	 * Takes another pane's zoom (split view, section 6.1): at once, a fit as a fit of this
	 * pane's own, keeping the top of the view where it is, and leading no other pane. Pages
	 * keep their pixels, stretched, until the zooms stop, as in a pinch zoom.
	 */
	async function followZoom(zoom: number, mode: ZoomMode) {
		if (!scroller || layout.pages.length === 0 || viewportW === 0) return;
		stopScrollGlide();
		stopZoomGlide();
		holdRendering();
		following++;
		try {
			await zoomAround(mode === 'custom' ? zoom : fitZoom(mode), mode, viewportW / 2, 0);
			followedTop = scroller?.scrollTop ?? null;
		} finally {
			following--;
		}
	}

	function zoomStep(direction: 1 | -1) {
		// A step during a glide goes on from where that glide was heading.
		glideZoom(stepZoom(zoomGlide?.to ?? tab.zoom, direction), 'custom', null);
	}

	/** The zoom that fits the row of `page` (the current page's by default). */
	function fitZoom(mode: 'fitWidth' | 'fitPage', page = tab.currentPage): number {
		const row = rowOf(page, tab.columns, tab.cover);
		const first = rowStart(row, tab.columns, tab.cover);
		const last = Math.min(tab.pages.length, rowStart(row + 1, tab.columns, tab.cover)) - 1;
		// The row's pages as they are turned on screen; past the end, the first page.
		const sizes = [];
		for (let i = first; i <= last; i++) sizes.push(turnedSize(tab.pages[i]!, rotationOf(i)));
		if (sizes.length === 0 && tab.pages[0]) sizes.push(turnedSize(tab.pages[0], rotationOf(0)));
		if (sizes.length === 0) return 1;
		// The largest of the row's pages, on each side of the middle in two-page modes.
		const size = {
			width: Math.max(...sizes.map((s) => s.width)),
			height: Math.max(...sizes.map((s) => s.height))
		};
		return mode === 'fitWidth'
			? fitWidthZoom(size, viewportW, tab.columns)
			: fitPageZoom(size, viewportW, viewportH, tab.columns);
	}

	function fit(mode: 'fitWidth' | 'fitPage') {
		// Fit page shows the whole current page: it ends at the page's top.
		glideZoom(fitZoom(mode), mode, null, mode === 'fitPage' ? tab.currentPage : null);
	}

	// While a pinch zoom goes on, pages keep the pixels they have, stretched, and are
	// rendered again once it pauses. Rendering at every step showed images of several zoom
	// levels at once and queued renders that were stale before they started.
	const ZOOM_SETTLE_MS = 200;
	let heldRenderZoom: number | null = $state(null);
	/** The zoom pages are rendered for: `tab.zoom`, or where a glide or pinch started. */
	const renderZoom = $derived(heldRenderZoom ?? tab.zoom);
	let settleTimer: ReturnType<typeof setTimeout> | undefined;

	// Rendering ahead: once the view has rested a moment, Rust renders the row after the
	// mounted pages and the row before them, and keeps the images (its image cache), so
	// they are there when scrolled or turned to. A scanned page takes a quarter of a second
	// to render, and came blank that long with every page read. The pages are not mounted:
	// the webview holds no pixels of them (ADR 0002). These requests wait for the pages on
	// screen, as thumbnails do, and are not made while the view moves from place to place
	// (a scrollbar drag), where each would be for a page already left.
	const AHEAD_DWELL_MS = 120;
	const aheadUrls = $derived.by(() => {
		if (viewportW === 0) return '';
		const scale = renderScale(renderZoom, dpr);
		return pagesBeside(layout, mounted[0], mounted[1])
			.map((index) => {
				const whole = wholePageScale(tab.pages[index]!, scale);
				const hidden = hiddenOnPage(tab, index);
				// A page with an annotation layer: rendering that renders its content too.
				const layer = tab.annotationLayerOn(index, hidden) ?? undefined;
				return pageUrl(tab.id, index, whole, tab.state.revision, undefined, undefined, hidden, layer);
			})
			.join(' ');
	});
	$effect(() => {
		if (!aheadUrls) return;
		const urls = aheadUrls.split(' ');
		let tickets: Ticket<void>[] = [];
		const timer = setTimeout(() => {
			tickets = urls.map((url, i) => scheduler.warm(url, AHEAD_PRIORITY + i));
			// A page that could not be rendered ahead says so when it is shown.
			for (const ticket of tickets) ticket.promise.catch(() => {});
		}, AHEAD_DWELL_MS);
		return () => {
			clearTimeout(timer);
			for (const ticket of tickets) ticket.cancel();
		};
	});

	let wheelFactor = 1;
	let wheelAnchor = { x: 0, y: 0 };
	let wheelFrame = 0;

	function onWheel(event: WheelEvent) {
		// The user's own scrolling takes over from a glide to an annotation.
		stopScrollGlide();
		hideTip();
		if (!scroller) return;
		if (!event.ctrlKey) {
			flipOnWheel(event);
			return;
		}
		// Ctrl+wheel and touchpad pinch (which arrives as Ctrl+wheel): zoom at the cursor.
		event.preventDefault();
		const rect = scroller.getBoundingClientRect();
		const anchor = { x: event.clientX - rect.left, y: event.clientY - rect.top };
		const factor = wheelZoomFactor(event.deltaY, event.deltaMode);
		if (isWheelNotch(event.deltaY, event.deltaMode)) {
			// A mouse wheel notch glides, going on from where a glide was heading.
			glideZoom((zoomGlide?.to ?? tab.zoom) * factor, 'custom', anchor);
			return;
		}
		// A pinch follows the fingers, once per frame however many events arrive; it takes
		// over from a glide, and its pages stay held until it pauses.
		stopZoomGlide();
		wheelFactor *= factor;
		wheelAnchor = anchor;
		holdRendering();
		if (!wheelFrame) wheelFrame = requestAnimationFrame(applyWheelZoom);
	}

	// In the single page and two-page modes, scrolling on past the end of the row shown turns
	// to the next one (section 6.1). A mouse wheel notch turns at once; a touchpad turns once
	// it has gone a notch's worth past the edge, and only once per gesture, so a fling stops
	// at the next row rather than racing through the document.
	const FLIP_DISTANCE_PX = 100;
	const FLIP_PAUSE_MS = 200;
	let flipDistance = 0;
	let flipHeld = false;
	let flipTimer: ReturnType<typeof setTimeout> | undefined;

	function flipOnWheel(event: WheelEvent) {
		if (tab.continuous || !scroller || event.deltaY === 0 || event.shiftKey) return;
		const down = event.deltaY > 0;
		const max = scroller.scrollHeight - scroller.clientHeight;
		const atEdge = down ? scroller.scrollTop >= max - 1 : scroller.scrollTop <= 1;
		if (!atEdge) {
			flipDistance = 0;
			return;
		}
		event.preventDefault();
		clearTimeout(flipTimer);
		flipTimer = setTimeout(() => {
			flipDistance = 0;
			flipHeld = false;
		}, FLIP_PAUSE_MS);
		if (flipHeld) return;
		const notch = isWheelNotch(event.deltaY, event.deltaMode);
		flipDistance += Math.abs(event.deltaMode === 1 ? event.deltaY * 33 : event.deltaY);
		if (!notch && flipDistance < FLIP_DISTANCE_PX) return;
		flipDistance = 0;
		flipHeld = !notch;
		void flip(down ? 1 : -1);
	}

	/** Keeps the current render zoom until zooming has paused. */
	function holdRendering() {
		heldRenderZoom ??= tab.zoom;
		clearTimeout(settleTimer);
		settleTimer = setTimeout(() => (heldRenderZoom = null), ZOOM_SETTLE_MS);
	}

	function applyWheelZoom() {
		wheelFrame = 0;
		const factor = wheelFactor;
		wheelFactor = 1;
		void zoomAround(tab.zoom * factor, 'custom', wheelAnchor.x, wheelAnchor.y);
	}

	// ----- pointer: text selection and annotation tools (section 6.5) -----

	/** What a pointer drag is doing. Coordinates are page points of `page`. */
	type Gesture =
		// `grabbed`: where a side of the selection was taken (client pixels), until the pointer
		// moves from there.
		// `unit`: the word or line a double- or triple-click selected, which stays selected, and
		// `by` what it is: the drag selects on by whole words or lines.
		| {
				kind: 'text';
				anchor: Caret;
				grabbed?: { x: number; y: number } | null;
				unit?: [Caret, Caret];
				by?: SelectionUnit;
		  }
		// An annotation pressed where a drag does something else (section 6.5): let go there
		// (`at`, client pixels), it is selected; dragged, the press becomes `then`. With the
		// Select tool, one that doesn't move, over text, whose drag selects the text; with
		// another tool, any, whose drag does what the tool does.
		| { kind: 'press'; page: number; hit: Annotation; at: { x: number; y: number }; then: Gesture }
		| { kind: 'move'; page: number; id: number; start: [number, number]; box: Box; handle: Handle | null; moved: boolean }
		| { kind: 'ink'; page: number; points: number[] }
		| { kind: 'area'; page: number; start: [number, number] }
		| { kind: 'textbox'; page: number; start: [number, number] };

	let gesture: Gesture | null = null;
	/** A drag is under way: the floating bars wait until it ends. */
	let dragging = $state(false);
	let lastPointer = { x: 0, y: 0 };
	let autoScroll = 0;

	/** Screen pixels to page points at the current zoom. */
	function px(n: number) {
		return n / (tab.zoom * CSS_PX_PER_PT);
	}

	/** The annotations of `page` the pointer can reach: those drawn on it, so not the ones
	 * the annotation list filters out. */
	function targets(page: number): Annotation[] {
		return tab.hitTargets(page).filter((a) => !hiddenByList(tab, a));
	}

	function pageUnder(clientX: number, clientY: number): number {
		const rect = scroller!.getBoundingClientRect();
		return Math.max(0, pageAt(layout, clientX - rect.left + scrollLeft, clientY - rect.top + scrollTop, contentW));
	}

	/** A point on `page`, kept on the page. */
	function pointOn(page: number, clientX: number, clientY: number): [number, number] {
		const [x, y] = toPage(page, clientX, clientY);
		const s = tab.pages[page]!;
		return [Math.min(s.width, Math.max(0, x)), Math.min(s.height, Math.max(0, y))];
	}

	function caretAt(clientX: number, clientY: number, nearest: boolean): Caret | null {
		const page = pageUnder(clientX, clientY);
		const text = tab.text(page);
		if (!text) {
			void tab.loadText(page);
			return null;
		}
		const [x, y] = toPage(page, clientX, clientY);
		return hitTest(text, x, y, nearest);
	}

	function begin(event: PointerEvent, g: Gesture) {
		gesture = g;
		dragging = true;
		event.preventDefault();
		scroller!.focus({ preventScroll: true });
		scroller!.setPointerCapture(event.pointerId);
		lastPointer = { x: event.clientX, y: event.clientY };
		autoScroll = requestAnimationFrame(autoScrollStep);
	}

	/** Starts a text selection (any tool that works on text): a double-click selects a word,
	 * a triple-click a line. */
	function beginText(event: PointerEvent, clicks: number): boolean {
		const caret = caretAt(event.clientX, event.clientY, false);
		// Shift extends what is selected, or selects from the text cursor.
		const from = tab.selection?.anchor ?? tab.caret;
		tab.caret = null;
		if (!caret) {
			tab.selection = null;
			return false;
		}
		const text = tab.text(caret.page)!;
		let anchor = caret;
		let unit: [Caret, Caret] | undefined;
		let by: SelectionUnit | undefined;
		if (clicks === 2 || clicks === 3) {
			by = clicks === 2 ? 'word' : 'line';
			unit = by === 'word' ? wordAt(text, caret) : lineAt(text, caret);
			tab.selection = { anchor: unit[0], focus: unit[1] };
			anchor = unit[0];
		} else if (event.shiftKey && from) {
			anchor = from;
			tab.selection = { anchor, focus: caret };
		} else {
			tab.selection = { anchor: caret, focus: caret };
		}
		begin(event, { kind: 'text', anchor, unit, by });
		return true;
	}

	/** How close (screen pixels) the pointer must be to a side of the selection to take it. */
	const EDGE_REACH_PX = 5;

	/** The side of the text selection at a point on `page` (page points), for the tools that
	 * select text. */
	function selectionEdgeAt(page: number, x: number, y: number): SelectionEdge | null {
		const sel = tab.selection;
		const text = tab.text(page);
		if (!sel || !text || !(tools.tool === 'select' || isMarkupTool(tools.tool))) return null;
		const [start, end] = ordered(sel.anchor, sel.focus);
		return edgeAt(text, start, end, x, y, px(EDGE_REACH_PX));
	}

	/**
	 * Takes the side of the selection under the pointer, to extend or reduce the selection
	 * from there (section 6.1): the drag moves that side, and the other one stays. Only a
	 * single press does: a double-click still selects a word, and Shift extends as ever.
	 */
	function beginEdge(event: PointerEvent, page: number, x: number, y: number, clicks: number): boolean {
		const sel = tab.selection;
		if (!sel || clicks !== 1 || event.shiftKey) return false;
		const edge = selectionEdgeAt(page, x, y);
		if (!edge) return false;
		const [start, end] = ordered(sel.anchor, sel.focus);
		tab.selectedAnnotation = null;
		overEdge = edge;
		begin(event, {
			kind: 'text',
			anchor: edge.side === 'start' ? end : start,
			grabbed: { x: event.clientX, y: event.clientY }
		});
		return true;
	}

	/** Selects an annotation the Select tool clicked. A click shows the comment panel of one
	 * with a comment (again, after Esc closed it); without one, its bar. While the annotation
	 * list shows the annotation, its row there has both, and the page neither. */
	function pickAnnotation(page: number, hit: Annotation, clicks: number) {
		tab.selectAnnotation(page, hit.id);
		if (clicks !== 2 && showsComment(hit)) setPanel(tab, true);
	}

	/** Whether `hit` on `page` is the selected annotation. */
	function isSelected(page: number, hit: Annotation): boolean {
		const sel = tab.selectedAnnotation;
		return sel?.page === page && sel.id === hit.id;
	}

	/**
	 * A press on annotation `hit` (at page point x, y). A click selects it as an object with
	 * any tool but the pen, which draws over annotations (section 6.5). With the Select tool, a drag moves one that moves, and selects
	 * the text under one that doesn't; with another tool, a drag from one not selected does
	 * what the tool does, and the selected one is moved as with the Select tool. False when
	 * the press is left to the tool.
	 */
	function pressAnnotation(event: PointerEvent, page: number, hit: Annotation, x: number, y: number, clicks: number): boolean {
		const tool = tools.tool;
		const caps = capabilities(hit, tab.flags.canAnnotate);
		const at = { x: event.clientX, y: event.clientY };
		if (tool !== 'select' && !isSelected(page, hit)) {
			// Shift is the tool's (extending the text selection). A double-click's first click
			// has selected the annotation, so its second is the Select tool's, below.
			if (clicks !== 1 || event.shiftKey) return false;
			let then: Gesture | null = null;
			if (isMarkupTool(tool)) {
				const caret = caretAt(event.clientX, event.clientY, false);
				if (caret) then = { kind: 'text', anchor: caret };
			} else if (tool === 'freeText') {
				then = { kind: 'textbox', page, start: [x, y] };
			}
			if (then) {
				begin(event, { kind: 'press', page, hit, at, then });
			} else {
				// Nothing to drag (a note is placed with a click; a markup tool away from text).
				pickAnnotation(page, hit, 1);
				event.preventDefault();
				scroller!.focus({ preventScroll: true });
			}
			return true;
		}
		// One that doesn't move, over text: a click selects it, a drag selects the text
		// (decided once the pointer moves or is let go).
		const caret = clicks === 1 && !caps.move && !event.shiftKey ? caretAt(event.clientX, event.clientY, false) : null;
		if (caret) {
			begin(event, { kind: 'press', page, hit, at, then: { kind: 'text', anchor: caret } });
			return true;
		}
		pickAnnotation(page, hit, clicks);
		if (clicks === 2) {
			// A double-click opens what the annotation says for editing: a text box's text in
			// place, anything else's note in the annotation list when that shows it, otherwise
			// in its comment panel.
			event.preventDefault();
			if (hit.kind === 'freeText' && caps.text) openTextEditor(hit);
			else openComment(tab, caps.text);
		} else if (caps.move) {
			begin(event, { kind: 'move', page, id: hit.id, start: [x, y], box: [...hit.bounds], handle: null, moved: false });
		} else {
			event.preventDefault();
			scroller!.focus({ preventScroll: true });
		}
		return true;
	}

	const clickCounter = new ClickCounter();

	// ----- links (section 6.1) -----

	/** The link at a point of `page` (page points), once the page's text is in. */
	function linkUnder(page: number, x: number, y: number): PageLink | null {
		const text = tab.text(page);
		return text ? linkAt(text.links, x, y) : null;
	}

	/** A link the Select tool pressed, and where (client pixels): let go there, it is
	 * followed; dragged from there, text is selected as anywhere else. */
	let linkPress: { link: PageLink; page: number; x: number; y: number } | null = null;

	function onPointerDown(event: PointerEvent) {
		stopScrollGlide();
		hideTip();
		if (event.button !== 0 || !scroller) return;
		const target = event.target as HTMLElement;
		if (target.closest('[data-annotation-editor], [data-floating-bar], [data-annotation-panel]')) return;
		const clicks = clickCounter.count(event);
		keyMade = false;
		if (tab.draft?.kind === 'text') {
			// A click outside the text box being typed finishes it.
			void commitTextDraft(tab);
			event.preventDefault();
			return;
		}
		if (!target.closest('.page')) {
			tab.selection = null;
			tab.caret = null;
			tab.selectedAnnotation = null;
			return;
		}
		const page = pageUnder(event.clientX, event.clientY);
		const [x, y] = pointOn(page, event.clientX, event.clientY);
		const tool = tools.tool;
		if (tool !== 'select' && refuseIfLocked(tab)) return;

		// Alt with a markup tool draws an area highlight, and the pen draws, over annotations too.
		const area = isMarkupTool(tool) && event.altKey;
		if (!area && tool !== 'ink') {
			// The selected annotation is an object whatever the tool: its corner handles resize
			// it, a drag moves it, a double-click opens its text.
			const selected = tab.selectedAnnotationInfo;
			if (selected && selected.page === page && capabilities(selected, tab.flags.canAnnotate).resize) {
				const handle = handleAt(selected.bounds, x, y, px(6));
				if (handle) {
					begin(event, { kind: 'move', page, id: selected.id, start: [x, y], box: [...selected.bounds], handle, moved: false });
					return;
				}
			}
			// A side of the selected text comes before an annotation under it.
			if (beginEdge(event, page, x, y, clicks)) return;
			const hit = annotationAt(targets(page), x, y, px(4));
			if (hit && pressAnnotation(event, page, hit, x, y, clicks)) return;
		}
		if (tool === 'select') {
			tab.selectedAnnotation = null;
			const link = clicks === 1 && !event.shiftKey ? linkUnder(page, x, y) : null;
			linkPress = link ? { link, page, x: event.clientX, y: event.clientY } : null;
			beginText(event, clicks);
			return;
		}

		tab.selectedAnnotation = null;
		if (isMarkupTool(tool)) {
			if (area) {
				tab.selection = null;
				tab.draft = { kind: 'area', page, box: [x, y, x, y] };
				begin(event, { kind: 'area', page, start: [x, y] });
			} else {
				beginText(event, clicks);
			}
		} else if (tool === 'ink') {
			tab.selection = null;
			tab.draft = { kind: 'ink', page, points: [x, y] };
			begin(event, { kind: 'ink', page, points: [x, y] });
		} else if (tool === 'note') {
			event.preventDefault();
			// The icon's top-left corner at the click, kept on the page: where the icon is as
			// the page is shown, which on a page turned on screen is another corner of it.
			const s = tab.pages[page]!;
			const turn = rotationOf(page);
			const shown = turnedSize(s, turn);
			const [sx, sy] = turnPoint(x, y, turn, s.width, s.height);
			const corner = [Math.min(sx, shown.width - 20), Math.min(sy, shown.height - 20)] as const;
			const at = unturnBox([corner[0], corner[1], corner[0] + 20, corner[1] + 20], turn, s.width, s.height);
			void create(tab, 'note', [{ page, body: { tool: 'note', x: at[0], y: at[1], text: '' } }]).then((change) => {
				if (change) {
					tools.tool = 'select';
					openInspector(tab, true);
					if (change.created != null) madeForComment(tab, page, change.created, change.state.revision);
				}
			});
		} else if (tool === 'freeText') {
			tab.selection = null;
			tab.draft = { kind: 'area', page, box: [x, y, x, y] };
			begin(event, { kind: 'textbox', page, start: [x, y] });
		}
	}

	/** Follows the pointer during a drag. */
	function dragTo(clientX: number, clientY: number) {
		let g = gesture;
		if (!g) return;
		if (g.kind === 'press') {
			// Pressed on an annotation: dragged from there, it does what the drag would do
			// anywhere else.
			if (Math.hypot(clientX - g.at.x, clientY - g.at.y) < 3) return;
			const next = g.then;
			tab.selectedAnnotation = null;
			tab.caret = null;
			tab.selection = next.kind === 'text' ? { anchor: next.anchor, focus: next.anchor } : null;
			if (next.kind === 'textbox') tab.draft = { kind: 'area', page: next.page, box: [...next.start, ...next.start] };
			g = gesture = next;
		}
		if (g.kind === 'text') {
			if (g.grabbed) {
				// A side taken and let go where it was leaves the selection as it is.
				if (Math.hypot(clientX - g.grabbed.x, clientY - g.grabbed.y) < 3) return;
				g.grabbed = null;
			}
			const focus = caretAt(clientX, clientY, true);
			if (!focus) return;
			const text = tab.text(focus.page);
			if (g.unit && g.by && text) {
				// The word or line stays whole; a drag out of it selects on from its far side, by
				// whole words or lines.
				tab.selection = extendByUnit(text, g.unit, focus, g.by);
				return;
			}
			tab.selection = { anchor: g.anchor, focus };
			return;
		}
		const [x, y] = pointOn(g.page, clientX, clientY);
		if (g.kind === 'move') {
			const dx = x - g.start[0];
			const dy = y - g.start[1];
			if (!g.moved && Math.hypot(dx, dy) < px(3)) return;
			g.moved = true;
			const s = tab.pages[g.page]!;
			const box = g.handle ? resizeBox(g.box, g.handle, dx, dy, px(8)) : clampBox(moveBox(g.box, dx, dy), s.width, s.height);
			tab.draft = { kind: 'move', page: g.page, id: g.id, box };
		} else if (g.kind === 'ink') {
			const n = g.points.length;
			if (Math.hypot(x - g.points[n - 2]!, y - g.points[n - 1]!) < px(1)) return;
			g.points.push(x, y);
			tab.draft = { kind: 'ink', page: g.page, points: [...g.points] };
		} else if (g.kind === 'area' || g.kind === 'textbox') {
			tab.draft = { kind: 'area', page: g.page, box: [g.start[0], g.start[1], x, y] };
		}
	}

	let cursorFrame = 0;
	let overText = $state(false);
	/** The view has the keyboard focus: the text cursor shows only then, as in a text field. */
	let focused = $state(false);
	/** What the pointer is over with the Select tool: an annotation that moves, or one that doesn't. */
	let overAnnotation = $state<'move' | 'select' | null>(null);
	/** The link the pointer is on, which the Select tool would follow: it shows the hand,
	 * and (unless Settings say otherwise) the link's area is tinted. */
	let hoveredLink = $state.raw<{ page: number; rect: PageLink['rect'] } | null>(null);
	const overLink = $derived(hoveredLink !== null);

	function leavePage() {
		hoveredLink = null;
		hideTip();
	}
	/** The side of the text selection the pointer is on, or is dragging. */
	let overEdge = $state<SelectionEdge | null>(null);
	/** The cursor for it: the arrows along the line as it is on screen. */
	const edgeCursor = $derived.by(() => {
		if (!overEdge || !tab.selection) return null;
		const [start, end] = ordered(tab.selection.anchor, tab.selection.focus);
		const rotation = rotationOf((overEdge.side === 'start' ? start : end).page);
		const turned = rotation === 90 || rotation === 270;
		return overEdge.vertical !== turned ? 'ns' : 'ew';
	});

	function onPointerMove(event: PointerEvent) {
		lastPointer = { x: event.clientX, y: event.clientY };
		if (gesture) {
			dragTo(event.clientX, event.clientY);
			return;
		}
		if (cursorFrame) return;
		cursorFrame = requestAnimationFrame(() => {
			cursorFrame = 0;
			if (!scroller) return;
			const target = document.elementFromPoint(lastPointer.x, lastPointer.y);
			if (!target?.closest('.page')) {
				overText = false;
				overAnnotation = null;
				overEdge = null;
				leavePage();
				return;
			}
			const page = pageUnder(lastPointer.x, lastPointer.y);
			const [x, y] = toPage(page, lastPointer.x, lastPointer.y);
			overEdge = selectionEdgeAt(page, x, y);
			overAnnotation = null;
			const hit = annotationAt(targets(page), x, y, px(4));
			// With another tool, one not selected shows that a click selects it; the pen draws.
			if (hit && tools.tool !== 'ink') {
				const asSelect = tools.tool === 'select' || isSelected(page, hit);
				overAnnotation = asSelect && capabilities(hit, tab.flags.canAnnotate).move ? 'move' : 'select';
			}
			const rect = scroller.getBoundingClientRect();
			tipPointer = { x: lastPointer.x - rect.left + scrollLeft, y: lastPointer.y - rect.top + scrollTop };
			const text = tab.text(page);
			overText = !!text && isOverText(text, x, y);
			// An annotation or a side of the selection comes before a link under it.
			const link = tools.tool === 'select' && !hit && !overEdge && text ? linkAt(text.links, x, y) : null;
			// The same link keeps its object, so the page is not drawn again for every move.
			if (!link) hoveredLink = null;
			else if (hoveredLink?.page !== page || hoveredLink.rect !== link.rect) hoveredLink = { page, rect: link.rect };
			hoverTip(hit, link, page);
		});
	}

	// ----- the tooltip on what is under the pointer: an annotation's comment (section 6.5),
	// or where a link leads (section 6.1) -----

	/** What a tooltip is about: the annotation `id`, or a link, on `page`. */
	type TipSource = { page: number; id: number } | { page: number; link: PageLink };

	/** What the tooltip that shows is about. */
	let tip = $state.raw<TipSource | null>(null);
	/** Where the pointer is, in the scrolled content: the tooltip follows it. */
	let tipPointer = $state({ x: 0, y: 0 });
	let pendingTip: TipSource | null = null;
	let tipTimer: ReturnType<typeof setTimeout> | undefined;

	/** Where a link leads, for its tooltip; nothing when Settings turn these off. */
	function linkTipText(link: PageLink): string {
		return app.settings?.linkPreviews === false ? '' : linkPreview(link.target, tab.displayLabels);
	}

	/** The comment a tooltip would show for `a`: none for a text box, whose text is on the page. */
	function tipText(a: Annotation | null): string {
		return a && a.kind !== 'freeText' ? a.contents.trim() : '';
	}

	/** Shows the comment of `hit`, or where `link` on `page` leads, after a moment; from one
	 * annotation or link to another, at once. */
	function hoverTip(hit: Annotation | null, link: PageLink | null, page: number) {
		const next: TipSource | null =
			hit && tipText(hit) ? { page: hit.page, id: hit.id } : link && linkTipText(link) ? { page, link } : null;
		const same = (a: TipSource | null) =>
			a !== null &&
			next !== null &&
			a.page === next.page &&
			('id' in a ? 'id' in next && a.id === next.id : 'link' in next && a.link === next.link);
		if (same(tip) || same(pendingTip)) return;
		clearTimeout(tipTimer);
		pendingTip = null;
		if (!next) {
			tip = null;
		} else if (tip) {
			tip = next;
		} else {
			// The pointer rests on it this long first (Settings).
			const delay = app.settings?.tooltipDelayMs ?? DEFAULT_TIP_DELAY_MS;
			pendingTip = next;
			tipTimer = setTimeout(() => {
				tip = pendingTip;
				pendingTip = null;
			}, delay);
		}
	}

	function hideTip() {
		clearTimeout(tipTimer);
		pendingTip = null;
		tip = null;
	}

	/** The tooltip's text and where it goes, while nothing is being dragged or typed. */
	const tipShown = $derived.by(() => {
		if (!tip || dragging || tab.draft || !isShown(layout, tip.page)) return null;
		if ('link' in tip) {
			const leads = linkTipText(tip.link);
			return leads ? { text: leads } : null;
		}
		const a = tab.annotation(tip.page, tip.id);
		const text = tipText(a);
		return a && text ? { text } : null;
	});

	async function onPointerUp(event: PointerEvent) {
		// A link pressed and let go where it was is followed, once the press is dealt with.
		const pressed = linkPress;
		linkPress = null;
		const followed =
			pressed && event.type === 'pointerup' && Math.hypot(event.clientX - pressed.x, event.clientY - pressed.y) < 3
				? pressed
				: null;
		if (!gesture) {
			if (followed) follow(tab, followed.link.target, { kind: 'page', page: followed.page });
			return;
		}
		dragTo(event.clientX, event.clientY);
		// Read after the drag's last move, which can turn a press into a text selection.
		const g = gesture as Gesture;
		gesture = null;
		dragging = false;
		cancelAnimationFrame(autoScroll);
		if (scroller?.hasPointerCapture(event.pointerId)) scroller.releasePointerCapture(event.pointerId);
		const tool = tools.tool;
		switch (g.kind) {
			case 'text': {
				if (g.grabbed) break;
				const sel = tab.selection;
				if (followed || (sel && compareCarets(sel.anchor, sel.focus) === 0)) {
					tab.selection = null;
					// A click in the text puts the text cursor there.
					if (!followed && sel && caretTool(tool) && event.type === 'pointerup') tab.caret = sel.anchor;
				} else if (sel) {
					// The selection bar goes above this point.
					const page = pageUnder(event.clientX, event.clientY);
					const [x, y] = pointOn(page, event.clientX, event.clientY);
					release = { anchor: sel.anchor, focus: sel.focus, page, x, y };
					if (isMarkupTool(tool)) await markSelection(tab, tool);
				}
				break;
			}
			case 'press': {
				// Let go where it was pressed: a click on the annotation.
				if (event.type === 'pointerup') pickAnnotation(g.page, g.hit, 1);
				break;
			}
			case 'move': {
				const draft = tab.draft;
				if (g.moved && draft?.kind === 'move') {
					await update(tab, g.page, g.id, { bounds: draft.box });
				}
				tab.draft = null;
				break;
			}
			case 'ink': {
				await create(tab, 'ink', [
					{ page: g.page, body: { tool: 'ink', strokes: [g.points], width: tools.style('ink').width } }
				]);
				// The pen stays: a drawing is often several strokes.
				tab.selectedAnnotation = null;
				tab.draft = null;
				break;
			}
			case 'area': {
				const box = normalizeBox(tab.draft?.kind === 'area' ? tab.draft.box : [0, 0, 0, 0]);
				tab.draft = null;
				if (isMarkupTool(tool) && box[2] - box[0] > px(3) && box[3] - box[1] > px(3)) {
					await createMarkup(tab, tool, [
						{ page: g.page, body: { tool: 'markup', kind: tool, quads: boxQuad(box, rotationOf(g.page)), note: null } }
					]);
				}
				break;
			}
			case 'textbox': {
				const style = tools.style('freeText');
				const s = tab.pages[g.page]!;
				// The box is typed upright as the page is shown: worked out where a page turned
				// on screen shows it, and brought back to the page.
				const turn = rotationOf(g.page);
				const shown = turnedSize(s, turn);
				const dragged = turnBox(tab.draft?.kind === 'area' ? tab.draft.box : [0, 0, 0, 0], turn, s.width, s.height);
				const [startX] = turnPoint(g.start[0], g.start[1], turn, s.width, s.height);
				// A click makes a box 200 pt wide; a drag sets the width.
				const width = dragged[2] - dragged[0] > px(16) ? dragged[2] - dragged[0] : Math.min(200, shown.width - startX);
				const x0 = Math.min(startX, dragged[0]);
				const typed: Box = [x0, dragged[1], x0 + Math.max(width, px(16)), dragged[1] + style.fontSize * 1.2];
				const box = unturnBox(typed, turn, s.width, s.height);
				tab.draft = { kind: 'text', page: g.page, id: null, box, turn, text: '', fontSize: style.fontSize, color: style.color };
				break;
			}
		}
		if (followed) follow(tab, followed.link.target, { kind: 'page', page: followed.page });
	}

	function openTextEditor(a: Annotation) {
		tab.draft = {
			kind: 'text',
			page: a.page,
			id: a.id,
			box: [...a.bounds],
			turn: normalizeRotation(a.turn),
			text: a.contents,
			fontSize: a.fontSize ?? 12,
			color: a.color ?? '#000000'
		};
	}

	// ----- floating bars (section 6.5) -----

	/** A rectangle in page points (unrotated) on page `index`, as an area of the content. */
	function contentArea(index: number, r: readonly number[]): Area {
		const b = pageBox(index);
		const [ax, ay] = toBox(index, r[0]!, r[1]!);
		const [bx, by] = toBox(index, r[2]!, r[3]!);
		return {
			x0: b.left + Math.min(ax, bx),
			y0: b.top + Math.min(ay, by),
			x1: b.left + Math.max(ax, bx),
			y1: b.top + Math.max(ay, by)
		};
	}

	const view: Area = $derived({ x0: scrollLeft, y0: scrollTop, x1: scrollLeft + viewportW, y1: scrollTop + viewportH });
	/** The quick tools over selected text: none while their bar is switched off. */
	const quickTools = $derived(app.settings?.quickBar === false ? [] : (app.settings?.quickTools ?? DEFAULT_QUICK_TOOLS));

	/** The selection whose quick tools were closed (after Copy): it stays selected, without
	 * the bar. */
	let barClosedFor: DocTab['selection'] = $state.raw(null);

	/** Where the pointer was released after selecting text (page points), and for which
	 * selection. */
	let release: { anchor: Caret; focus: Caret; page: number; x: number; y: number } | null = $state(null);

	/**
	 * Where the selection bar goes: above the point where the pointer was released, or
	 * below it if there is no room; for a selection the pointer didn't make, next to its
	 * last line, on the side it went.
	 */
	const selectionAnchor = $derived.by(() => {
		if (dragging || tools.tool !== 'select') return null;
		if (!quickTools.some((t) => quickToolAllowed(t, tab.flags, tab.canEditBookmarks))) return null;
		void tab.textVersion;
		const sel = tab.selection;
		if (!sel || sel === barClosedFor) return null;
		const order = compareCarets(sel.anchor, sel.focus);
		const text = tab.text(sel.focus.page);
		if (order === 0 || !text || !isShown(layout, sel.focus.page)) return null;
		const [start, end] = ordered(sel.anchor, sel.focus);
		const rects = selectionRects(text, start, end);
		const line = order < 0 ? rects.at(-1) : rects[0];
		if (!line) return null;
		const lineArea = contentArea(sel.focus.page, line);
		const r = release;
		if (r && isShown(layout, r.page) && compareCarets(r.anchor, sel.anchor) === 0 && compareCarets(r.focus, sel.focus) === 0) {
			const at = contentArea(r.page, [r.x, r.y, r.x, r.y]);
			return { area: releaseAnchor(lineArea, { x: at.x0, y: at.y0 }), prefer: 'above' as const };
		}
		return { area: lineArea, prefer: order < 0 ? ('below' as const) : ('above' as const) };
	});

	/** The selected annotation shows its comment (with the bar's controls) rather than its
	 * bar. */
	const commentOpen = $derived(panelOpen(tab));
	/** The annotation list shows the selected annotation: its row there has the comment and
	 * the controls, so the page shows neither the comment panel nor the bar. */
	const inList = $derived(listShowsSelected(tab));
	const showPanel = $derived(commentOpen && !inList);

	/** The selected annotation, for its bar, while it isn't being dragged or typed in, and
	 * while neither its comment panel nor the annotation list shows its controls. */
	const barAnnotation = $derived.by(() => {
		const a = tab.selectedAnnotationInfo;
		if (!a || dragging || menuOpen || tab.draft?.kind === 'text' || commentOpen || inList || !isShown(layout, a.page))
			return null;
		return { annotation: a, area: contentArea(a.page, a.bounds) };
	});

	// Each newly selected annotation decides for itself: its comment panel if it has a
	// comment, otherwise its bar. Kept from then on, so emptying the comment leaves the panel
	// open; set before this runs (the list opens the panel as it selects), it stays.
	const selKey = $derived(selectedKey(tab));
	$effect(() => {
		const key = selKey;
		untrack(() => {
			const a = tab.selectedAnnotationInfo;
			if (key && a && app.annotationPanel?.key !== key) app.annotationPanel = { key, open: showsComment(a) };
		});
	});

	/** Where the selected annotation is, for its comment panel while that shows. */
	const panelAnchor = $derived.by(() => {
		const a = tab.selectedAnnotationInfo;
		if (!a || !showPanel || !isShown(layout, a.page)) return null;
		return contentArea(a.page, a.bounds);
	});

	/** Scrolls while something is dragged past the top or bottom edge. */
	function autoScrollStep() {
		if (!gesture || !scroller) return;
		const rect = scroller.getBoundingClientRect();
		const edge = 32;
		// A side of the selection pressed near the top or bottom scrolls nothing until it moves.
		const pressed = (gesture.kind === 'text' && !!gesture.grabbed) || gesture.kind === 'press';
		let dy = 0;
		if (!pressed && lastPointer.y < rect.top + edge) dy = -Math.min(40, rect.top + edge - lastPointer.y);
		else if (!pressed && lastPointer.y > rect.bottom - edge) dy = Math.min(40, lastPointer.y - (rect.bottom - edge));
		if (dy !== 0) {
			scroller.scrollTop += dy;
			onScroll();
			dragTo(lastPointer.x, lastPointer.y);
		}
		autoScroll = requestAnimationFrame(autoScrollStep);
	}

	/**
	 * The keys that scroll, in the single page and two-page modes: past the end of the row
	 * shown they turn to the next one; Home and End go to the first and last; Left and Right
	 * turn when the row is not wider than the view. True if the key turned.
	 */
	function flipOnKey(event: KeyboardEvent): boolean {
		if (tab.continuous || !scroller || event.altKey) return false;
		const max = scroller.scrollHeight - scroller.clientHeight;
		const atTop = scroller.scrollTop <= 1;
		const atBottom = scroller.scrollTop >= max - 1;
		const narrow = scroller.scrollWidth <= scroller.clientWidth;
		const forward =
			((event.key === 'PageDown' || event.key === 'ArrowDown' || (event.key === ' ' && !event.shiftKey)) && atBottom) ||
			(event.key === 'ArrowRight' && narrow);
		const back =
			((event.key === 'PageUp' || event.key === 'ArrowUp' || (event.key === ' ' && event.shiftKey)) && atTop) ||
			(event.key === 'ArrowLeft' && narrow);
		if (event.key === 'Home' || event.key === 'End') {
			const row = event.key === 'Home' ? 0 : rowCount(tab.pages.length, tab.columns, tab.cover) - 1;
			if (row === shownRow) return false;
			void showRow(row, event.key === 'Home' ? 'top' : 'bottom');
			return true;
		}
		if (event.ctrlKey || (!forward && !back)) return false;
		void flip(forward ? 1 : -1);
		return true;
	}

	// ----- the text selection from the keyboard (section 6.1) -----

	/** Where Shift+Up and Shift+Down are heading, across lines of other lengths: the point
	 * they set out from, for as long as the selection ends where they left it. */
	let lineGoal: { focus: Caret; point: [number, number] } | null = null;
	/** Presses are taken one after another: a step may wait for a page's text. */
	let keySteps: Promise<void> = Promise.resolve();

	/**
	 * Shift with an arrow key moves the end of the selected text (the side last dragged or
	 * moved; the other one stays): Left and Right by a character, Up and Down by a line, and
	 * with Ctrl too, by a word and by a paragraph. True if the key was taken.
	 */
	function extendOnKey(event: KeyboardEvent): boolean {
		if (event.altKey || event.metaKey || !(tab.selection || tab.caret)) return false;
		// Without Shift, the keys move the text cursor; they scroll while text is selected.
		if (!event.shiftKey && tab.selection) return false;
		const sideways = event.key === 'ArrowLeft' || event.key === 'ArrowRight';
		if (!sideways && event.key !== 'ArrowUp' && event.key !== 'ArrowDown') return false;
		const direction = event.key === 'ArrowRight' || event.key === 'ArrowDown' ? 1 : -1;
		const step: CaretStep = sideways ? (event.ctrlKey ? 'word' : 'character') : event.ctrlKey ? 'paragraph' : 'line';
		const extend = event.shiftKey;
		keySteps = keySteps.then(() => (extend ? extendSelection(direction, step) : moveTextCaret(direction, step)));
		return true;
	}

	/** The selection as it is, or the text cursor as a selection of nothing. */
	function selectionOrCaret(): { anchor: Caret; focus: Caret } | null {
		return tab.selection ?? (tab.caret ? { anchor: tab.caret, focus: tab.caret } : null);
	}

	/** Where Shift+Up and Shift+Down (or Up and Down, for the text cursor) head from `from`. */
	function lineGoalFrom(from: Caret, step: CaretStep): [number, number] | undefined {
		if (step !== 'line') return undefined;
		if (lineGoal && compareCarets(lineGoal.focus, from) === 0) return lineGoal.point;
		const line = tab.text(from.page)?.lines[from.line];
		return line ? caretPoint(line, from.offset) : undefined;
	}

	/** True if a selection made by the keys with a markup tool waits to be marked: that
	 * happens when Shift is let go, as a drag is marked when the pointer is (section 6.5). */
	let keyMade = false;

	async function extendSelection(direction: 1 | -1, step: CaretStep) {
		const sel = selectionOrCaret();
		if (!sel) return;
		const { anchor, focus: from } = sel;
		const source = (page: number) => tab.loadText(page);
		const goal = lineGoalFrom(from, step);
		// Never nothing selected: a step that would end on the other side goes on past it, and
		// the selection turns over.
		const empty = (c: Caret) => selectionIsEmpty(tab.textMap(), ...ordered(anchor, c));
		let focus = await moveCaret(source, tab.pageCount, from, direction, step, goal);
		if (empty(focus)) {
			focus = await moveCaret(source, tab.pageCount, focus, direction, step, goal);
			if (empty(focus)) return;
		}
		// Something else changed the selection while a page's text was read.
		const now = selectionOrCaret();
		if (!now || compareCarets(now.anchor, anchor) !== 0 || compareCarets(now.focus, from) !== 0) return;
		tab.selection = { anchor, focus };
		tab.caret = null;
		if (isMarkupTool(tools.tool)) keyMade = true;
		lineGoal = goal ? { focus, point: goal } : null;
		const text = tab.text(focus.page);
		const box = text ? caretBox(text, focus) : null;
		if (box) await keepInView(focus.page, box);
	}

	/** Moves the text cursor as the arrow keys do in a text: Left and Right by a character
	 * (the end of a line and the start of the next are two places here), Up and Down by a
	 * line, and with Ctrl, by a word and by a paragraph. */
	async function moveTextCaret(direction: 1 | -1, step: CaretStep) {
		const from = tab.caret;
		if (!from) return;
		const goal = lineGoalFrom(from, step);
		let to = await moveCaret((page) => tab.loadText(page), tab.pageCount, from, direction, step, goal);
		if (step === 'character' && (to.page !== from.page || to.line !== from.line)) {
			const length = tab.text(to.page)?.lines[to.line]?.chars.length ?? 0;
			to = { ...to, offset: direction > 0 ? 0 : length };
		}
		if (!tab.caret || compareCarets(tab.caret, from) !== 0 || tab.selection) return;
		tab.caret = to;
		lineGoal = goal ? { focus: to, point: goal } : null;
		const text = tab.text(to.page);
		const box = text ? caretBox(text, to) : null;
		if (box) await keepInView(to.page, box);
	}

	/** Shift let go: a markup tool marks what the keys selected, and the text cursor stays
	 * where they left it, so the keys can go on from there. */
	function onKeyUp(event: KeyboardEvent) {
		if (event.key !== 'Shift' || !keyMade) return;
		keyMade = false;
		const tool = tools.tool;
		keySteps = keySteps.then(async () => {
			const sel = tab.selection;
			if (!isMarkupTool(tool) || tools.tool !== tool || !sel || compareCarets(sel.anchor, sel.focus) === 0) return;
			const at = sel.focus;
			await markSelection(tab, tool);
			// Unless the new markup opened its comment (Settings).
			if (!tab.selection && !tab.selectedAnnotation) tab.caret = at;
		});
	}

	/** The tools that select text, and so have a text cursor: Select and the markup tools. */
	function caretTool(tool: typeof tools.tool): boolean {
		return tool === 'select' || isMarkupTool(tool);
	}

	// Another tool takes the text cursor away; a selection made by the keys is marked only
	// by the markup tool it was made with.
	$effect(() => {
		const tool = tools.tool;
		untrack(() => {
			keyMade = false;
			if (!caretTool(tool)) tab.caret = null;
		});
	});

	/** Scrolls just far enough to show a rectangle of page `index` (page points). */
	async function keepInView(index: number, rect: readonly number[]) {
		const showing = showRowOf(index);
		if (showing) await showing;
		if (!scroller || !layout.pages[index]) return;
		const area = contentArea(index, rect);
		const margin = 32;
		let top: number | null = null;
		let left: number | null = null;
		if (area.y0 < scrollTop + margin) top = area.y0 - margin;
		else if (area.y1 > scrollTop + viewportH - margin) top = area.y1 + margin - viewportH;
		if (area.x0 < scrollLeft + margin) left = area.x0 - margin;
		else if (area.x1 > scrollLeft + viewportW - margin) left = area.x1 + margin - viewportW;
		glideScroll(top, left, 0);
	}

	function onKeyDown(event: KeyboardEvent) {
		// Keys in the comment panel are its own: Delete there doesn't delete the annotation.
		if ((event.target as HTMLElement).closest('[data-annotation-panel]')) return;
		if (event.target === scroller && extendOnKey(event)) {
			event.preventDefault();
			return;
		}
		if (event.target === scroller && flipOnKey(event)) {
			event.preventDefault();
			return;
		}
		if ((event.key === 'Delete' || event.key === 'Backspace') && tab.selectedAnnotation && !event.ctrlKey) {
			const { page, id } = tab.selectedAnnotation;
			const a = tab.annotation(page, id);
			if (a && capabilities(a, tab.flags.canAnnotate).delete) {
				event.preventDefault();
				void remove(tab, page, id);
			}
		}
	}

	// ----- context menu -----

	function onContextMenu(event: MouseEvent) {
		const contextPage = pageUnder(event.clientX, event.clientY);
		menuPoint = scroller
			? { x: event.clientX, y: event.clientY, left: scroller.scrollLeft, top: scroller.scrollTop }
			: null;
		// A right-click on an annotation selects it (unless text is selected, so Copy
		// selection still acts on the text); from the keyboard, the menu is for what is
		// already selected. Selected so, it opens neither its comment nor (while the menu
		// is open) its bar: the menu has what they would offer.
		if (!tab.selection && (event.target as HTMLElement).closest('.page')) {
			const [x, y] = toPage(contextPage, event.clientX, event.clientY);
			const hit = annotationAt(targets(contextPage), x, y, px(4));
			const sel = tab.selectedAnnotation;
			if (hit && (sel?.page !== contextPage || sel.id !== hit.id)) {
				app.annotationPanel = { key: `${tab.key}:${contextPage}:${hit.id}`, open: false };
				tab.selectAnnotation(contextPage, hit.id);
			}
		}
	}

	/** Whether the page's context menu is open. */
	let menuOpen = $state(false);
	/** Where the context menu was opened (client coordinates) and the scroll position then:
	 * the menu stays by that point of the page as the page scrolls. */
	let menuPoint: { x: number; y: number; left: number; top: number } | null = null;
	const menuAnchor = {
		getBoundingClientRect: () => {
			const p = menuPoint;
			const dx = p && scroller ? p.left - scroller.scrollLeft : 0;
			const dy = p && scroller ? p.top - scroller.scrollTop : 0;
			return DOMRect.fromRect({ x: (p?.x ?? 0) + dx, y: (p?.y ?? 0) + dy, width: 0, height: 0 });
		}
	};

	/** Closes the context menu once the point it was opened at scrolls out of the view. */
	function closeMenuOffscreen() {
		if (!menuOpen || !menuPoint || !scroller) return;
		const at = menuAnchor.getBoundingClientRect();
		const view = scroller.getBoundingClientRect();
		if (at.y < view.top || at.y > view.bottom || at.x < view.left || at.x > view.right) menuOpen = false;
	}

	const contextAnnotation = $derived(tab.selectedAnnotationInfo);

	// ----- lifecycle -----

	/** Takes the viewport's size; the first time, fits the zoom and goes to the start position. */
	function measure() {
		if (!scroller) return;
		const w = scroller.clientWidth;
		const h = scroller.clientHeight;
		scrollbarW = Math.max(0, scroller.offsetWidth - w);
		// What moves the viewer (the view bar wrapping, a banner, a pane) resizes it too.
		const at = scroller.getBoundingClientRect();
		originX = at.left + scroller.clientLeft;
		originY = at.top + scroller.clientTop;
		if (w === 0 || (w === viewportW && h === viewportH)) return;
		const first = viewportW === 0;
		// A fit mode re-fits on every frame of a pane sliding or being resized, or of the
		// window being resized: render once that settles, as for a wheel zoom.
		if (!first && tab.zoomMode !== 'custom') holdRendering();
		viewportW = w;
		viewportH = h;
		dpr = window.devicePixelRatio || 1;
		if (first) {
			if (tab.zoomMode !== 'custom') tab.zoom = fitZoom(tab.zoomMode);
			const pending = tab.pendingPosition;
			tab.pendingPosition = null;
			void tick().then(() => scrollToPosition(pending ?? { page: 0, offset: 0 }));
		}
	}

	onMount(() => {
		if (!scroller) return;
		// Measured now rather than on the observer's first report, which can come several
		// frames later: until then the first pages rendered at the default zoom, and again
		// once fitted.
		measure();
		const resize = new ResizeObserver(measure);
		resize.observe(scroller);
		scroller.addEventListener('wheel', onWheel, { passive: false });
		scroller.focus({ preventScroll: true });

		tab.viewer = {
			position,
			goTo,
			topLeft,
			goToPoint,
			visibleArea,
			centerOn,
			reveal,
			setZoom,
			zoomStep,
			fit,
			setMode: (mode, cover) => void setMode(mode, cover),
			turnPages,
			turn,
			focus: () => scroller?.focus({ preventScroll: true }),
			place,
			follow: (at) => void followPlace(at),
			followZoom
		};

		return () => {
			resize.disconnect();
			scroller?.removeEventListener('wheel', onWheel);
			tab.pendingPosition = position();
			tab.viewer = null;
			cancelAnimationFrame(autoScroll);
			cancelAnimationFrame(wheelFrame);
			stopZoomGlide();
			stopScrollGlide();
			clearTimeout(settleTimer);
			clearTimeout(tipTimer);
			clearTimeout(flipTimer);
		};
	});

	// In a fit mode, re-fit when the window resizes, the view rotates or a page is turned on
	// screen. Only those: the fit is computed from the current page, and re-fitting whenever
	// the current page or a page size changed would feed back into which page is current.
	// Fit width ignores height changes, such as a horizontal scrollbar appearing when a wide
	// page scrolls into view.
	let fittedFor: { width: number; height: number; rotation: number; turns: DocTab['pageTurns'] | null } = {
		width: 0,
		height: 0,
		rotation: -1,
		turns: null
	};
	$effect(() => {
		const size = { width: viewportW, height: viewportH, rotation: tab.rotation as number, turns: tab.pageTurns };
		untrack(() => {
			const last = fittedFor;
			fittedFor = size;
			if (size.width === 0 || tab.zoomMode === 'custom') return;
			const relevant =
				size.rotation !== last.rotation ||
				size.turns !== last.turns ||
				size.width !== last.width ||
				(tab.zoomMode === 'fitPage' && size.height !== last.height);
			if (!relevant) return;
			const zoom = fitZoom(tab.zoomMode);
			if (Math.abs(zoom - tab.zoom) > 1e-3) setZoomNow(zoom, tab.zoomMode);
		});
	});

	// ----- the selected text, for the system (section 6.1) -----

	// The text selection is drawn, not the page's own, so the system would see none: the
	// accessibility API's selected text (which translators and dictionaries such as Easydict
	// read, and screen readers announce), macOS's Edit > Copy and Services, Linux's primary
	// selection. A hidden copy of the selected text, selected, stands in for it while the view
	// has the focus. Not where the document forbids copying, and not for a selection over
	// many pages whose text isn't read yet.
	let mirror: HTMLDivElement | undefined = $state();
	let mirrored = $state('');
	/** Pages a selection may span whose text is read for the mirror. */
	const MIRROR_LOAD_PAGES = 10;
	let mirrorFrame = 0;

	function mirrorOwnsSelection(): boolean {
		const sel = document.getSelection();
		return !!mirror && !!sel?.anchorNode && mirror.contains(sel.anchorNode);
	}

	$effect(() => {
		const sel = tab.selection;
		const allowed = tab.flags.canCopy;
		void focused;
		cancelAnimationFrame(mirrorFrame);
		mirrorFrame = requestAnimationFrame(() => {
			untrack(() => void updateMirror(sel, allowed));
		});
		return () => cancelAnimationFrame(mirrorFrame);
	});
	onMount(() => () => {
		if (mirrorOwnsSelection()) document.getSelection()?.removeAllRanges();
	});

	async function updateMirror(sel: DocTab['selection'], allowed: boolean) {
		let text = '';
		if (sel && allowed) {
			const [start, end] = ordered(sel.anchor, sel.focus);
			let unread = false;
			for (let page = start.page; page <= end.page && !unread; page++) unread = !tab.text(page);
			if (!unread || end.page - start.page < MIRROR_LOAD_PAGES) text = await selectedText(tab);
			if (tab.selection !== sel) return;
		}
		mirrored = text;
		await tick();
		const domSel = document.getSelection();
		if (!mirror || !domSel) return;
		if (!text) {
			if (mirrorOwnsSelection()) domSel.removeAllRanges();
			return;
		}
		// Only while the view has the focus: a field being typed in keeps its own selection.
		const active = document.activeElement;
		if (active !== document.body && !scroller?.contains(active)) return;
		domSel.selectAllChildren(mirror);
	}

	/** The system's Copy (macOS's Edit > Copy) copies the selected text as Copy selection does. */
	function onCopy(event: ClipboardEvent) {
		if (!mirrored || !mirrorOwnsSelection() || !event.clipboardData) return;
		event.preventDefault();
		event.clipboardData.setData('text/plain', mirrored);
	}
</script>

<div class="relative h-full min-h-0 flex-1">
	<ContextMenu.Root bind:open={menuOpen}>
		<ContextMenu.Trigger>
			{#snippet child({ props })}
				<!-- The scroll container takes focus so the keyboard can scroll it (Page Up/Down,
				     arrows, Home/End), as a native document view does. -->
				<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
				<div
					{...props}
					bind:this={scroller}
					class="viewer-scroll h-full overflow-auto bg-canvas outline-none"
					class:cursor-text={overText && !overAnnotation && !overLink && !edgeCursor && (tools.tool === 'select' || isMarkupTool(tools.tool))}
					class:cursor-crosshair={!overAnnotation && (tools.tool === 'ink' || tools.tool === 'note' || tools.tool === 'freeText')}
					class:cursor-move={overAnnotation === 'move' && !edgeCursor}
					class:cursor-pointer={(overAnnotation === 'select' || overLink) && !edgeCursor}
					class:cursor-ew-resize={edgeCursor === 'ew'}
					class:cursor-ns-resize={edgeCursor === 'ns'}
					tabindex="0"
					role="document"
					aria-label="{tab.name}, {tab.pageCount} pages"
					onscroll={onScroll}
					onpointerdown={chain(props, 'onpointerdown', onPointerDown)}
					onkeydown={onKeyDown}
					onkeyup={onKeyUp}
					onfocus={() => (focused = true)}
					onblur={() => (focused = false)}
					onpointermove={chain(props, 'onpointermove', onPointerMove)}
					onpointerup={chain(props, 'onpointerup', onPointerUp)}
					onpointercancel={chain(props, 'onpointercancel', onPointerUp)}
					onpointerleave={chain(props, 'onpointerleave', leavePage)}
					oncontextmenu={chain(props, 'oncontextmenu', onContextMenu)}
					oncopy={onCopy}
				>
					<div bind:this={mirror} class="selection-mirror">{mirrored}</div>
					<div bind:this={content} class="relative" style:width="{contentW}px" style:height="{layout.totalHeight}px">
						{#each mountedPages as index (index)}
							{@const b = pageBox(index)}
							<PageView
								{tab}
								{index}
								size={tab.pages[index]!}
								left={b.left}
								top={b.top}
								width={b.width}
								height={b.height}
								zoom={tab.zoom}
								{renderZoom}
								zooming={heldRenderZoom !== null}
								rotation={rotationOf(index)}
								devicePixelRatio={dpr}
								origin={{ x: originX, y: originY }}
								linkHover={hoveredLink?.page === index && !dragging ? hoveredLink.rect : null}
								visibleRect={visibleRect(index)}
								onscreen={isOnscreen(index)}
								priority={priority(index)}
								caret={focused && tab.caret?.page === index && !tab.selection && !dragging ? tab.caret : null}
							/>
						{/each}
						{#if selectionAnchor}
							<SelectionBar
								{tab}
								anchor={selectionAnchor.area}
								prefer={selectionAnchor.prefer}
								{view}
								contentWidth={contentW}
								chosen={quickTools}
								onclose={(sel) => (barClosedFor = sel)}
							/>
						{/if}
						{#if barAnnotation}
							<AnnotationBar
								{tab}
								annotation={barAnnotation.annotation}
								anchor={barAnnotation.area}
								{view}
								contentWidth={contentW}
								onedittext={() => {
									const a = tab.selectedAnnotationInfo;
									if (a) openTextEditor(a);
								}}
							/>
						{/if}
						{#if panelAnchor}
							<AnnotationInspector {tab} anchor={panelAnchor} {view} contentWidth={contentW} />
						{/if}
						{#if tipShown}
							<CommentTip text={tipShown.text} pointer={tipPointer} {view} contentWidth={contentW} />
						{/if}
					</div>
				</div>
			{/snippet}
		</ContextMenu.Trigger>
		<ContextMenu.Portal>
			<ContextMenu.Content class="menu-content" customAnchor={menuAnchor} updatePositionStrategy="always">
				{#if contextAnnotation}
					{@const caps = capabilities(contextAnnotation, tab.flags.canAnnotate)}
					<ContextMenu.Item class="menu-item" onSelect={openProperties}>Properties</ContextMenu.Item>
					<ContextMenu.Item
						class="menu-item"
						disabled={!caps.delete}
						onSelect={() => void remove(tab, contextAnnotation.page, contextAnnotation.id)}
					>
						Delete annotation
						<span class="menu-shortcut">Del</span>
					</ContextMenu.Item>
					<ContextMenu.Separator class="menu-separator" />
					<ContextMenu.Item
						class="menu-item"
						disabled={!contextAnnotation.contents}
						onSelect={() => void copyComment(contextAnnotation.contents)}
					>
						Copy comment
					</ContextMenu.Item>
				{/if}
				<ContextMenu.Item class="menu-item" disabled={!copiesSelection(tab)} onSelect={() => void copyInContext(tab)}>
					Copy selection
					<span class="menu-shortcut">{app.shortcut('copy')}</span>
				</ContextMenu.Item>
			</ContextMenu.Content>
		</ContextMenu.Portal>
	</ContextMenu.Root>

	{#if marks}
		<!-- Over the scrollbar's track, letting the pointer through to it. -->
		<div
			class="pointer-events-none absolute top-0 right-0"
			style:width="{Math.max(scrollbarW, 6)}px"
			style:height="{viewportH}px"
			aria-hidden="true"
			data-search-marks
		>
			{#each marks.tops as top (top)}
				<div class="bg-search-marker absolute inset-x-0" style:top="{top}px" style:height="{MARK_PX}px"></div>
			{/each}
			{#if marks.current !== null}
				<div
					class="bg-search-marker-current absolute inset-x-0"
					style:top="{marks.current}px"
					style:height="{CURRENT_MARK_PX}px"
					data-search-mark-current
				></div>
			{/if}
		</div>
	{/if}
	{#if tab.search.open}
		<SearchBar {tab} />
	{/if}
</div>
