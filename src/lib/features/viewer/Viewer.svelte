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
		create,
		createMarkup,
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
		type Box,
		type Handle
	} from '#lib/features/annotations/geometry.ts';
	import SelectionBar from '#lib/features/annotations/SelectionBar.svelte';
	import { tools } from '#lib/features/annotations/state.svelte.ts';
	import { DEFAULT_QUICK_TOOLS, capabilities, isMarkupTool, showsComment } from '#lib/features/annotations/tools.ts';
	import type { Annotation } from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { copySelection } from './actions.ts';
	import type { ViewPosition } from './history.ts';
	import {
		CSS_PX_PER_PT,
		computeLayout,
		contentWidth,
		currentPage,
		pageAtY,
		pageLeft,
		pagesInRange
	} from './layout.ts';
	import PageView from './PageView.svelte';
	import SearchBar from './SearchBar.svelte';
	import {
		compareCarets,
		hitTest,
		isOverText,
		lineAt,
		ordered,
		selectionRects,
		wordAt,
		type Caret
	} from './selection.ts';
	import {
		ZOOM_GLIDE_MS,
		clampZoom,
		easeOut,
		fitPageZoom,
		fitWidthZoom,
		isWheelNotch,
		stepZoom,
		wheelZoomFactor,
		zoomBetween,
		type ZoomMode
	} from './zoom.ts';

	let { tab }: { tab: DocTab } = $props();

	let scroller: HTMLDivElement | undefined = $state();
	let scrollTop = $state(0);
	let scrollLeft = $state(0);
	let viewportW = $state(0);
	let viewportH = $state(0);
	let dpr = $state(typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1);

	const layout = $derived(computeLayout(tab.pages, tab.zoom, tab.rotation));
	const contentW = $derived(contentWidth(layout, viewportW));
	/** Pages mounted: the viewport plus a quarter screen above and below. Each mounted page
	 * holds its pixels in the webview, and rendering is fast enough that a wider margin
	 * only cost memory (ADR 0002). */
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
	}

	// ----- positions -----

	function position(): ViewPosition {
		if (!scroller || layout.pages.length === 0) return { page: 0, offset: 0 };
		const page = Math.max(0, pageAtY(layout, scrollTop + 1));
		const b = layout.pages[page]!;
		return { page, offset: Math.min(1, Math.max(0, (scrollTop - b.top) / b.height)) };
	}

	function scrollToPosition(p: ViewPosition) {
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
		scrollToPosition(p);
	}

	/** Maps a point in page points (unrotated) to CSS pixels in the rotated page box. */
	function toBox(index: number, x: number, y: number): [number, number] {
		const k = tab.zoom * CSS_PX_PER_PT;
		const s = tab.pages[index]!;
		const [W, H] = [s.width, s.height];
		switch (tab.rotation) {
			case 90:
				return [(H - y) * k, x * k];
			case 180:
				return [(W - x) * k, (H - y) * k];
			case 270:
				return [y * k, (W - x) * k];
			default:
				return [x * k, y * k];
		}
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
		const [W, H] = [s.width, s.height];
		switch (tab.rotation) {
			case 90:
				return [dy / k, H - dx / k];
			case 180:
				return [W - dx / k, H - dy / k];
			case 270:
				return [W - dy / k, dx / k];
			default:
				return [dx / k, dy / k];
		}
	}

	function topLeft(): { page: number; x: number; y: number } {
		if (!scroller || layout.pages.length === 0) return { page: 0, x: 0, y: 0 };
		let page = Math.max(0, pageAtY(layout, scrollTop + 1));
		// In the gap below a page, the next page is the one coming into view.
		const below = layout.pages[page]!;
		if (scrollTop >= below.top + below.height - 1 && page + 1 < layout.pages.length) page++;
		const b = pageBox(page);
		const clampX = (v: number) => Math.min(b.width, Math.max(0, v));
		const clampY = (v: number) => Math.min(b.height, Math.max(0, v));
		const x0 = clampX(scrollLeft - b.left);
		const x1 = clampX(scrollLeft + viewportW - b.left);
		const y0 = clampY(scrollTop - b.top);
		const y1 = clampY(scrollTop + viewportH - b.top);
		// The visible part's top-left corner in the page's own orientation (the view may be
		// rotated).
		const corners = [boxToPage(page, x0, y0), boxToPage(page, x1, y0), boxToPage(page, x0, y1), boxToPage(page, x1, y1)];
		return {
			page,
			x: Math.min(...corners.map((c) => c[0])),
			y: Math.min(...corners.map((c) => c[1]))
		};
	}

	function goToPoint(index: number, x: number | null, y: number | null) {
		if (!scroller) return;
		const page = Math.min(Math.max(0, index), layout.pages.length - 1);
		if (!layout.pages[page]) return;
		tab.history.push(position());
		const b = pageBox(page);
		const [bx, by] = toBox(page, x ?? 0, y ?? 0);
		// Place the point at the top-left corner of the page's content as it appears on
		// screen: with a rotated view, that corner is elsewhere in the box.
		let top: number;
		let left: number | null;
		switch (tab.rotation) {
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
		scroller.scrollTop = top;
		if (left !== null) scroller.scrollLeft = left;
		onScroll();
	}

	function reveal(index: number, rect: [number, number, number, number], options: { smooth?: boolean } = {}) {
		if (!scroller) return;
		const [ax, ay] = toBox(index, rect[0], rect[1]);
		const [bx, by] = toBox(index, rect[2], rect[3]);
		const b = pageBox(index);
		const top = b.top + Math.min(ay, by);
		const bottom = b.top + Math.max(ay, by);
		const left = b.left + Math.min(ax, bx);
		const right = b.left + Math.max(ax, bx);
		const toTop = top < scrollTop + 40 || bottom > scrollTop + viewportH - 40 ? (top + bottom) / 2 - viewportH / 2 : null;
		const toLeft = left < scrollLeft || right > scrollLeft + viewportW ? (left + right) / 2 - viewportW / 2 : null;
		glideScroll(toTop, toLeft, options.smooth ? motionMs(SCROLL_GLIDE_MS) : 0);
	}

	// ----- gliding scroll (section 6.5: going to an annotation picked in the list) -----

	const SCROLL_GLIDE_MS = 140;
	let scrollFrame = 0;

	/**
	 * Scrolls to `top` and `left` (null leaves that axis), over `duration` ms. A long way
	 * starts two screens from the target, so the pages in between are not all rendered.
	 */
	function glideScroll(top: number | null, left: number | null, duration: number) {
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
		const near = 2 * viewportH;
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
		};
		scrollFrame = requestAnimationFrame(frame);
	}

	function stopScrollGlide() {
		cancelAnimationFrame(scrollFrame);
		scrollFrame = 0;
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
		const page = Math.max(0, pageAtY(before, y));
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
		const next = computeLayout(tab.pages, z, tab.rotation);
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

	function zoomStep(direction: 1 | -1) {
		// A step during a glide goes on from where that glide was heading.
		glideZoom(stepZoom(zoomGlide?.to ?? tab.zoom, direction), 'custom', null);
	}

	function fitZoom(mode: 'fitWidth' | 'fitPage'): number {
		const size = tab.pages[tab.currentPage] ?? tab.pages[0];
		if (!size) return 1;
		return mode === 'fitWidth'
			? fitWidthZoom(size, viewportW, tab.rotation)
			: fitPageZoom(size, viewportW, viewportH, tab.rotation);
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
	let wheelFactor = 1;
	let wheelAnchor = { x: 0, y: 0 };
	let wheelFrame = 0;

	function onWheel(event: WheelEvent) {
		// The user's own scrolling takes over from a glide to an annotation.
		stopScrollGlide();
		hideTip();
		if (!event.ctrlKey || !scroller) return;
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
		| { kind: 'text'; anchor: Caret }
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

	function pageUnder(clientY: number): number {
		const rect = scroller!.getBoundingClientRect();
		return Math.max(0, pageAtY(layout, clientY - rect.top + scrollTop));
	}

	/** A point on `page`, kept on the page. */
	function pointOn(page: number, clientX: number, clientY: number): [number, number] {
		const [x, y] = toPage(page, clientX, clientY);
		const s = tab.pages[page]!;
		return [Math.min(s.width, Math.max(0, x)), Math.min(s.height, Math.max(0, y))];
	}

	function caretAt(clientX: number, clientY: number, nearest: boolean): Caret | null {
		const page = pageUnder(clientY);
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
		if (!caret) {
			tab.selection = null;
			return false;
		}
		const text = tab.text(caret.page)!;
		let anchor = caret;
		if (clicks === 2 || clicks === 3) {
			const [a, focus] = clicks === 2 ? wordAt(text, caret) : lineAt(text, caret);
			tab.selection = { anchor: a, focus };
			anchor = a;
		} else if (event.shiftKey && tab.selection) {
			anchor = tab.selection.anchor;
			tab.selection = { anchor, focus: caret };
		} else {
			tab.selection = { anchor: caret, focus: caret };
		}
		begin(event, { kind: 'text', anchor });
		return true;
	}

	const clickCounter = new ClickCounter();

	function onPointerDown(event: PointerEvent) {
		stopScrollGlide();
		hideTip();
		if (event.button !== 0 || !scroller) return;
		const target = event.target as HTMLElement;
		if (target.closest('[data-annotation-editor], [data-floating-bar], [data-annotation-panel]')) return;
		const clicks = clickCounter.count(event);
		if (tab.draft?.kind === 'text') {
			// A click outside the text box being typed finishes it.
			void commitTextDraft(tab);
			event.preventDefault();
			return;
		}
		if (!target.closest('.page')) {
			tab.selection = null;
			tab.selectedAnnotation = null;
			return;
		}
		const page = pageUnder(event.clientY);
		const [x, y] = pointOn(page, event.clientX, event.clientY);
		const tool = tools.tool;
		if (tool !== 'select' && refuseIfLocked(tab)) return;

		if (tool === 'select') {
			const selected = tab.selectedAnnotationInfo;
			if (selected && selected.page === page && capabilities(selected, tab.flags.canAnnotate).resize) {
				const handle = handleAt(selected.bounds, x, y, px(6));
				if (handle) {
					begin(event, { kind: 'move', page, id: selected.id, start: [x, y], box: [...selected.bounds], handle, moved: false });
					return;
				}
			}
			const hit = annotationAt(tab.hitTargets(page), x, y, px(4));
			if (hit) {
				tab.selectAnnotation(page, hit.id);
				// A click shows the comment panel of an annotation with a comment (again, after
				// Esc closed it); without one, its bar.
				if (clicks !== 2 && showsComment(hit)) setPanel(tab, true);
				const caps = capabilities(hit, tab.flags.canAnnotate);
				if (clicks === 2) {
					// A double-click opens what the annotation says for editing: a text box's
					// text in place, anything else's note in its comment panel when that shows,
					// else in the annotation list when that shows, otherwise in the panel.
					event.preventDefault();
					if (hit.kind === 'freeText' && caps.text) openTextEditor(hit);
					else openComment(tab, caps.text);
				} else if (caps.move) {
					begin(event, { kind: 'move', page, id: hit.id, start: [x, y], box: [...hit.bounds], handle: null, moved: false });
				} else {
					event.preventDefault();
					scroller.focus({ preventScroll: true });
				}
				return;
			}
			tab.selectedAnnotation = null;
			beginText(event, clicks);
			return;
		}

		tab.selectedAnnotation = null;
		if (isMarkupTool(tool)) {
			if (event.altKey) {
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
			// The icon's top-left corner at the click, kept on the page.
			const s = tab.pages[page]!;
			const at = [Math.min(x, s.width - 20), Math.min(y, s.height - 20)] as const;
			void create(tab, 'note', [{ page, body: { tool: 'note', x: at[0], y: at[1], text: '' } }]).then((change) => {
				if (change) {
					tools.tool = 'select';
					openInspector(tab, true);
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
		const g = gesture;
		if (!g) return;
		if (g.kind === 'text') {
			const focus = caretAt(clientX, clientY, true);
			if (focus) tab.selection = { anchor: g.anchor, focus };
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
		} else {
			tab.draft = { kind: 'area', page: g.page, box: [g.start[0], g.start[1], x, y] };
		}
	}

	let cursorFrame = 0;
	let overText = $state(false);
	/** What the pointer is over with the Select tool: an annotation that moves, or one that doesn't. */
	let overAnnotation = $state<'move' | 'select' | null>(null);

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
				hideTip();
				return;
			}
			const page = pageUnder(lastPointer.y);
			const [x, y] = toPage(page, lastPointer.x, lastPointer.y);
			overAnnotation = null;
			const hit = annotationAt(tab.hitTargets(page), x, y, px(4));
			if (hit && tools.tool === 'select') overAnnotation = capabilities(hit, tab.flags.canAnnotate).move ? 'move' : 'select';
			hoverTip(hit);
			const rect = scroller.getBoundingClientRect();
			tipPointer = { x: lastPointer.x - rect.left + scrollLeft, y: lastPointer.y - rect.top + scrollTop };
			const text = tab.text(page);
			overText = !!text && isOverText(text, x, y);
		});
	}

	// ----- the comment of the annotation under the pointer (section 6.5) -----

	/** The annotation whose comment shows. */
	let tip = $state<{ page: number; id: number } | null>(null);
	/** Where the pointer is, in the scrolled content: the tooltip follows it. */
	let tipPointer = $state({ x: 0, y: 0 });
	let pendingTip: { page: number; id: number } | null = null;
	let tipTimer: ReturnType<typeof setTimeout> | undefined;

	/** The comment a tooltip would show for `a`: none for a text box, whose text is on the page. */
	function tipText(a: Annotation | null): string {
		return a && a.kind !== 'freeText' ? a.contents.trim() : '';
	}

	/** Shows the comment of `hit` after a moment; from one annotation to another, at once. */
	function hoverTip(hit: Annotation | null) {
		const next = hit && tipText(hit) ? { page: hit.page, id: hit.id } : null;
		const same = (a: typeof next) => a !== null && next !== null && a.page === next.page && a.id === next.id;
		if (same(tip) || same(pendingTip)) return;
		clearTimeout(tipTimer);
		pendingTip = null;
		if (!next) {
			tip = null;
		} else if (tip) {
			tip = next;
		} else {
			// The pointer rests on the annotation this long first (Settings).
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
		if (!tip || dragging || tab.draft || !layout.pages[tip.page]) return null;
		const a = tab.annotation(tip.page, tip.id);
		const text = tipText(a);
		return a && text ? { text } : null;
	});

	async function onPointerUp(event: PointerEvent) {
		const g = gesture;
		if (!g) return;
		dragTo(event.clientX, event.clientY);
		gesture = null;
		dragging = false;
		cancelAnimationFrame(autoScroll);
		if (scroller?.hasPointerCapture(event.pointerId)) scroller.releasePointerCapture(event.pointerId);
		const tool = tools.tool;
		switch (g.kind) {
			case 'text': {
				const sel = tab.selection;
				if (sel && compareCarets(sel.anchor, sel.focus) === 0) {
					tab.selection = null;
				} else if (sel) {
					// The selection bar goes above this point.
					const page = pageUnder(event.clientY);
					const [x, y] = pointOn(page, event.clientX, event.clientY);
					release = { anchor: sel.anchor, focus: sel.focus, page, x, y };
					if (isMarkupTool(tool)) await markSelection(tab, tool);
				}
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
						{ page: g.page, body: { tool: 'markup', kind: tool, quads: boxQuad(box), note: null } }
					]);
				}
				break;
			}
			case 'textbox': {
				const dragged = normalizeBox(tab.draft?.kind === 'area' ? tab.draft.box : [0, 0, 0, 0]);
				const style = tools.style('freeText');
				const s = tab.pages[g.page]!;
				// A click makes a box 200 pt wide; a drag sets the width.
				const width = dragged[2] - dragged[0] > px(16) ? dragged[2] - dragged[0] : Math.min(200, s.width - g.start[0]);
				const x0 = Math.min(g.start[0], dragged[0]);
				const box: Box = [x0, dragged[1], x0 + Math.max(width, px(16)), dragged[1] + style.fontSize * 1.2];
				tab.draft = { kind: 'text', page: g.page, id: null, box, text: '', fontSize: style.fontSize, color: style.color };
				break;
			}
		}
	}

	function openTextEditor(a: Annotation) {
		tab.draft = {
			kind: 'text',
			page: a.page,
			id: a.id,
			box: [...a.bounds],
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
	const quickTools = $derived(app.settings?.quickTools ?? DEFAULT_QUICK_TOOLS);

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
		if (!sel) return null;
		const order = compareCarets(sel.anchor, sel.focus);
		const text = tab.text(sel.focus.page);
		if (order === 0 || !text || !layout.pages[sel.focus.page]) return null;
		const [start, end] = ordered(sel.anchor, sel.focus);
		const rects = selectionRects(text, start, end);
		const line = order < 0 ? rects.at(-1) : rects[0];
		if (!line) return null;
		const lineArea = contentArea(sel.focus.page, line);
		const r = release;
		if (r && layout.pages[r.page] && compareCarets(r.anchor, sel.anchor) === 0 && compareCarets(r.focus, sel.focus) === 0) {
			const at = contentArea(r.page, [r.x, r.y, r.x, r.y]);
			return { area: releaseAnchor(lineArea, { x: at.x0, y: at.y0 }), prefer: 'above' as const };
		}
		return { area: lineArea, prefer: order < 0 ? ('below' as const) : ('above' as const) };
	});

	/** The selected annotation shows its comment panel (which has the bar's controls)
	 * rather than its bar. */
	const showPanel = $derived(panelOpen(tab));

	/** The selected annotation, for its bar, while it isn't being dragged or typed in, and
	 * while it doesn't show its comment panel. */
	const barAnnotation = $derived.by(() => {
		const a = tab.selectedAnnotationInfo;
		if (!a || dragging || tab.draft?.kind === 'text' || showPanel || !layout.pages[a.page]) return null;
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
		if (!a || !showPanel || !layout.pages[a.page]) return null;
		return contentArea(a.page, a.bounds);
	});

	/** Scrolls while something is dragged past the top or bottom edge. */
	function autoScrollStep() {
		if (!gesture || !scroller) return;
		const rect = scroller.getBoundingClientRect();
		const edge = 32;
		let dy = 0;
		if (lastPointer.y < rect.top + edge) dy = -Math.min(40, rect.top + edge - lastPointer.y);
		else if (lastPointer.y > rect.bottom - edge) dy = Math.min(40, lastPointer.y - (rect.bottom - edge));
		if (dy !== 0) {
			scroller.scrollTop += dy;
			onScroll();
			dragTo(lastPointer.x, lastPointer.y);
		}
		autoScroll = requestAnimationFrame(autoScrollStep);
	}

	function onKeyDown(event: KeyboardEvent) {
		// Keys in the comment panel are its own: Delete there doesn't delete the annotation.
		if ((event.target as HTMLElement).closest('[data-annotation-panel]')) return;
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
		const contextPage = pageUnder(event.clientY);
		// A right-click on an annotation selects it (unless text is selected, so Copy still
		// acts on the text); from the keyboard, the menu is for what is already selected.
		if (!tab.selection && (event.target as HTMLElement).closest('.page')) {
			const [x, y] = toPage(contextPage, event.clientX, event.clientY);
			const hit = annotationAt(tab.hitTargets(contextPage), x, y, px(4));
			if (hit) tab.selectAnnotation(contextPage, hit.id);
		}
	}

	const contextAnnotation = $derived(tab.selectedAnnotationInfo);

	// ----- lifecycle -----

	/** Takes the viewport's size; the first time, fits the zoom and goes to the start position. */
	function measure() {
		if (!scroller) return;
		const w = scroller.clientWidth;
		const h = scroller.clientHeight;
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
			reveal,
			setZoom,
			zoomStep,
			fit,
			focus: () => scroller?.focus({ preventScroll: true })
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
		};
	});

	// In a fit mode, re-fit when the window resizes or the view rotates. Only those: the fit
	// is computed from the current page, and re-fitting whenever the current page or a page
	// size changed would feed back into which page is current.
	// Fit width ignores height changes, such as a horizontal scrollbar appearing when a wide
	// page scrolls into view.
	let fittedFor = { width: 0, height: 0, rotation: -1 };
	$effect(() => {
		const size = { width: viewportW, height: viewportH, rotation: tab.rotation as number };
		untrack(() => {
			const last = fittedFor;
			fittedFor = size;
			if (size.width === 0 || tab.zoomMode === 'custom') return;
			const relevant =
				size.rotation !== last.rotation ||
				size.width !== last.width ||
				(tab.zoomMode === 'fitPage' && size.height !== last.height);
			if (!relevant) return;
			const zoom = fitZoom(tab.zoomMode);
			if (Math.abs(zoom - tab.zoom) > 1e-3) setZoomNow(zoom, tab.zoomMode);
		});
	});
</script>

<div class="relative h-full min-h-0 flex-1">
	<ContextMenu.Root>
		<ContextMenu.Trigger>
			{#snippet child({ props })}
				<!-- The scroll container takes focus so the keyboard can scroll it (Page Up/Down,
				     arrows, Home/End), as a native document view does. -->
				<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
				<div
					{...props}
					bind:this={scroller}
					class="viewer-scroll h-full overflow-auto bg-canvas outline-none"
					class:cursor-text={overText && !overAnnotation && (tools.tool === 'select' || isMarkupTool(tools.tool))}
					class:cursor-crosshair={tools.tool === 'ink' || tools.tool === 'note' || tools.tool === 'freeText'}
					class:cursor-move={overAnnotation === 'move'}
					class:cursor-pointer={overAnnotation === 'select'}
					tabindex="0"
					role="document"
					aria-label="{tab.name}, {tab.pageCount} pages"
					onscroll={onScroll}
					onpointerdown={chain(props, 'onpointerdown', onPointerDown)}
					onkeydown={onKeyDown}
					onpointermove={chain(props, 'onpointermove', onPointerMove)}
					onpointerup={chain(props, 'onpointerup', onPointerUp)}
					onpointercancel={chain(props, 'onpointercancel', onPointerUp)}
					onpointerleave={chain(props, 'onpointerleave', hideTip)}
					oncontextmenu={chain(props, 'oncontextmenu', onContextMenu)}
				>
					<div class="relative" style:width="{contentW}px" style:height="{layout.totalHeight}px">
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
								rotation={tab.rotation}
								devicePixelRatio={dpr}
								visibleRect={visibleRect(index)}
								onscreen={isOnscreen(index)}
								priority={priority(index)}
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
			<ContextMenu.Content class="menu-content">
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
				{/if}
				<ContextMenu.Item
					class="menu-item"
					disabled={!tab.selection}
					onSelect={() => void copySelection(tab)}
				>
					Copy
					<span class="menu-shortcut">Ctrl+C</span>
				</ContextMenu.Item>
			</ContextMenu.Content>
		</ContextMenu.Portal>
	</ContextMenu.Root>

	{#if tab.search.open}
		<SearchBar {tab} />
	{/if}
</div>
