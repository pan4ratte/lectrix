<script lang="ts">
	// The page canvas: a virtualized, continuously scrolling column of pages. Only pages
	// within one screen of the viewport are mounted (section 3); the rest are space.
	import { ContextMenu } from 'bits-ui';
	import { onMount, tick, untrack } from 'svelte';

	import { chain } from '#lib/components/chain.ts';
	import { commitTextDraft, create, refuseIfLocked, remove, update } from '#lib/features/annotations/actions.ts';
	import {
		annotationAt,
		boxQuad,
		clampBox,
		handleAt,
		moveBox,
		normalizeBox,
		resizeBox,
		selectionRanges,
		type Box,
		type Handle
	} from '#lib/features/annotations/geometry.ts';
	import { tools } from '#lib/features/annotations/state.svelte.ts';
	import { capabilities, isMarkupTool } from '#lib/features/annotations/tools.ts';
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
	import { compareCarets, hitTest, isOverText, lineAt, ordered, wordAt, type Caret } from './selection.ts';
	import { clampZoom, fitPageZoom, fitWidthZoom, type ZoomMode } from './zoom.ts';

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
	 * only cost memory (Phase 2 WebView2 memory investigation). */
	const mounted = $derived(pagesInRange(layout, scrollTop - viewportH * 0.25, scrollTop + viewportH * 1.25));
	const mountedPages = $derived.by(() => {
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

	function reveal(index: number, rect: [number, number, number, number]) {
		if (!scroller) return;
		const [ax, ay] = toBox(index, rect[0], rect[1]);
		const [bx, by] = toBox(index, rect[2], rect[3]);
		const b = pageBox(index);
		const top = b.top + Math.min(ay, by);
		const bottom = b.top + Math.max(ay, by);
		const left = b.left + Math.min(ax, bx);
		const right = b.left + Math.max(ax, bx);
		if (top < scrollTop + 40 || bottom > scrollTop + viewportH - 40) {
			scroller.scrollTop = (top + bottom) / 2 - viewportH / 2;
		}
		if (left < scrollLeft || right > scrollLeft + viewportW) {
			scroller.scrollLeft = (left + right) / 2 - viewportW / 2;
		}
		onScroll();
	}

	// ----- zoom -----

	/** Zooms so that the content point under (ax, ay) in the viewport stays there. */
	async function zoomAround(zoom: number, mode: ZoomMode, ax: number, ay: number) {
		if (!scroller || layout.pages.length === 0) return;
		const before = layout;
		const y = scrollTop + ay;
		const page = Math.max(0, pageAtY(before, y));
		const b = before.pages[page]!;
		const fy = (y - b.top) / b.height;
		const left = pageLeft(before, page, contentW);
		const fx = (scrollLeft + ax - left) / b.width;

		tab.zoomMode = mode;
		tab.zoom = clampZoom(zoom);
		await tick();
		const after = layout.pages[page]!;
		const afterLeft = pageLeft(layout, page, contentW);
		scroller.scrollTop = after.top + fy * after.height - ay;
		scroller.scrollLeft = afterLeft + fx * after.width - ax;
		onScroll();
	}

	function setZoom(zoom: number, mode: ZoomMode) {
		void zoomAround(zoom, mode, viewportW / 2, viewportH / 2);
	}

	function fitZoom(mode: 'fitWidth' | 'fitPage'): number {
		const size = tab.pages[tab.currentPage] ?? tab.pages[0];
		if (!size) return 1;
		return mode === 'fitWidth'
			? fitWidthZoom(size, viewportW, tab.rotation)
			: fitPageZoom(size, viewportW, viewportH, tab.rotation);
	}

	function fit(mode: 'fitWidth' | 'fitPage') {
		if (mode === 'fitPage') {
			const page = tab.currentPage;
			tab.zoomMode = mode;
			tab.zoom = fitZoom(mode);
			void tick().then(() => scrollToPosition({ page, offset: 0 }));
		} else {
			setZoom(fitZoom(mode), mode);
		}
	}

	function onWheel(event: WheelEvent) {
		if (!event.ctrlKey || !scroller) return;
		// Ctrl+wheel and touchpad pinch (which arrives as Ctrl+wheel): zoom at the cursor.
		event.preventDefault();
		const rect = scroller.getBoundingClientRect();
		const delta = event.deltaMode === 1 ? event.deltaY * 33 : event.deltaY;
		// About 1.25x per wheel notch (Chromium reports a notch as 100-150 px).
		const factor = Math.exp(-delta * 0.0018);
		void zoomAround(tab.zoom * factor, 'custom', event.clientX - rect.left, event.clientY - rect.top);
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
		event.preventDefault();
		scroller!.focus({ preventScroll: true });
		scroller!.setPointerCapture(event.pointerId);
		lastPointer = { x: event.clientX, y: event.clientY };
		autoScroll = requestAnimationFrame(autoScrollStep);
	}

	/** Starts a text selection (any tool that works on text). */
	function beginText(event: PointerEvent): boolean {
		const caret = caretAt(event.clientX, event.clientY, false);
		if (!caret) {
			tab.selection = null;
			return false;
		}
		const text = tab.text(caret.page)!;
		let anchor = caret;
		if (event.detail === 2 || event.detail === 3) {
			const [a, focus] = event.detail === 2 ? wordAt(text, caret) : lineAt(text, caret);
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

	function onPointerDown(event: PointerEvent) {
		if (event.button !== 0 || !scroller) return;
		const target = event.target as HTMLElement;
		if (target.closest('[data-annotation-editor]')) return;
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
			const hit = annotationAt(tab.annotationsOn(page), x, y, px(4));
			if (hit) {
				tab.selectAnnotation(page, hit.id);
				const caps = capabilities(hit, tab.flags.canAnnotate);
				if (event.detail === 2 && hit.kind === 'freeText' && caps.text) {
					openTextEditor(hit);
					event.preventDefault();
				} else if (caps.move) {
					begin(event, { kind: 'move', page, id: hit.id, start: [x, y], box: [...hit.bounds], handle: null, moved: false });
				} else {
					event.preventDefault();
					scroller.focus({ preventScroll: true });
				}
				return;
			}
			tab.selectedAnnotation = null;
			beginText(event);
			return;
		}

		tab.selectedAnnotation = null;
		if (isMarkupTool(tool)) {
			if (event.altKey) {
				tab.selection = null;
				tab.draft = { kind: 'area', page, box: [x, y, x, y] };
				begin(event, { kind: 'area', page, start: [x, y] });
			} else {
				beginText(event);
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
					app.focusNoteText = true;
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
				return;
			}
			const page = pageUnder(lastPointer.y);
			const [x, y] = toPage(page, lastPointer.x, lastPointer.y);
			overAnnotation = null;
			if (tools.tool === 'select') {
				const hit = annotationAt(tab.annotationsOn(page), x, y, px(4));
				if (hit) overAnnotation = capabilities(hit, tab.flags.canAnnotate).move ? 'move' : 'select';
			}
			const text = tab.text(page);
			overText = !!text && isOverText(text, x, y);
		});
	}

	async function onPointerUp(event: PointerEvent) {
		const g = gesture;
		if (!g) return;
		dragTo(event.clientX, event.clientY);
		gesture = null;
		cancelAnimationFrame(autoScroll);
		if (scroller?.hasPointerCapture(event.pointerId)) scroller.releasePointerCapture(event.pointerId);
		const tool = tools.tool;
		switch (g.kind) {
			case 'text': {
				const sel = tab.selection;
				if (sel && compareCarets(sel.anchor, sel.focus) === 0) {
					tab.selection = null;
				} else if (sel && isMarkupTool(tool)) {
					const [start, end] = ordered(sel.anchor, sel.focus);
					const pages = selectionRanges(tab.textMap(), start, end);
					tab.selection = null;
					await create(
						tab,
						tool,
						pages.map((p) => ({ page: p.page, body: { tool: 'textMarkup', kind: tool, ranges: p.ranges, note: null } }))
					);
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
					await create(tab, tool, [
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

	let contextPage = $state(0);

	function onContextMenu(event: MouseEvent) {
		contextPage = pageUnder(event.clientY);
	}

	// ----- lifecycle -----

	onMount(() => {
		if (!scroller) return;
		const resize = new ResizeObserver(() => {
			if (!scroller) return;
			const first = viewportW === 0;
			viewportW = scroller.clientWidth;
			viewportH = scroller.clientHeight;
			dpr = window.devicePixelRatio || 1;
			if (first) {
				if (tab.zoomMode !== 'custom') tab.zoom = fitZoom(tab.zoomMode);
				const pending = tab.pendingPosition;
				tab.pendingPosition = null;
				void tick().then(() => scrollToPosition(pending ?? { page: 0, offset: 0 }));
			}
		});
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
			fit,
			focus: () => scroller?.focus({ preventScroll: true })
		};

		return () => {
			resize.disconnect();
			scroller?.removeEventListener('wheel', onWheel);
			tab.pendingPosition = position();
			tab.viewer = null;
			cancelAnimationFrame(autoScroll);
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
			if (Math.abs(zoom - tab.zoom) > 1e-3) setZoom(zoom, tab.zoomMode);
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
								rotation={tab.rotation}
								devicePixelRatio={dpr}
								visibleRect={visibleRect(index)}
								onscreen={isOnscreen(index)}
								priority={priority(index)}
							/>
						{/each}
					</div>
				</div>
			{/snippet}
		</ContextMenu.Trigger>
		<ContextMenu.Portal>
			<ContextMenu.Content class="menu-content">
				<ContextMenu.Item
					class="menu-item"
					disabled={!tab.selection}
					onSelect={() => void copySelection(tab)}
				>
					Copy
					<span class="menu-shortcut">Ctrl+C</span>
				</ContextMenu.Item>
				<ContextMenu.Separator class="menu-separator" />
				<ContextMenu.Item
					class="menu-item"
					disabled={!tab.flags.canAssemble}
					onSelect={() => void app.rotatePages(tab, [contextPage], 90)}
				>
					Rotate page clockwise
				</ContextMenu.Item>
				<ContextMenu.Item
					class="menu-item"
					disabled={!tab.flags.canAssemble}
					onSelect={() => void app.rotatePages(tab, [contextPage], -90)}
				>
					Rotate page counter-clockwise
				</ContextMenu.Item>
			</ContextMenu.Content>
		</ContextMenu.Portal>
	</ContextMenu.Root>

	{#if tab.search.open}
		<SearchBar {tab} />
	{/if}
</div>
