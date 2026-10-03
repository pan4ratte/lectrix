<script lang="ts">
	// The page canvas: a virtualized, continuously scrolling column of pages. Only pages
	// within one screen of the viewport are mounted (section 3); the rest are space.
	import { ContextMenu } from 'bits-ui';
	import { onMount, tick, untrack } from 'svelte';

	import { chain } from '#lib/components/chain.ts';
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
	import { hitTest, isOverText, lineAt, wordAt, type Caret } from './selection.ts';
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
	/** Pages mounted: the viewport plus one screen above and below. */
	const mounted = $derived(pagesInRange(layout, scrollTop - viewportH, scrollTop + 2 * viewportH));
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
		const dx = clientX - rect.left + scrollLeft - b.left;
		const dy = clientY - rect.top + scrollTop - b.top;
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

	// ----- selection -----

	let dragging: { anchor: Caret } | null = null;
	let lastPointer = { x: 0, y: 0 };
	let autoScroll = 0;

	function pageUnder(clientY: number): number {
		const rect = scroller!.getBoundingClientRect();
		return Math.max(0, pageAtY(layout, clientY - rect.top + scrollTop));
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

	function onPointerDown(event: PointerEvent) {
		if (event.button !== 0 || !scroller) return;
		const target = event.target as HTMLElement;
		if (!target.closest('.page')) {
			tab.selection = null;
			return;
		}
		const caret = caretAt(event.clientX, event.clientY, false);
		if (!caret) {
			tab.selection = null;
			return;
		}
		event.preventDefault();
		scroller.focus({ preventScroll: true });
		const text = tab.text(caret.page)!;
		if (event.detail === 2 || event.detail === 3) {
			const [anchor, focus] = event.detail === 2 ? wordAt(text, caret) : lineAt(text, caret);
			tab.selection = { anchor, focus };
			dragging = { anchor };
		} else if (event.shiftKey && tab.selection) {
			tab.selection = { anchor: tab.selection.anchor, focus: caret };
			dragging = { anchor: tab.selection.anchor };
		} else {
			tab.selection = { anchor: caret, focus: caret };
			dragging = { anchor: caret };
		}
		scroller.setPointerCapture(event.pointerId);
		lastPointer = { x: event.clientX, y: event.clientY };
		autoScroll = requestAnimationFrame(autoScrollStep);
	}

	function extendTo(clientX: number, clientY: number) {
		if (!dragging) return;
		const focus = caretAt(clientX, clientY, true);
		if (focus) tab.selection = { anchor: dragging.anchor, focus };
	}

	let cursorFrame = 0;
	let overText = $state(false);

	function onPointerMove(event: PointerEvent) {
		lastPointer = { x: event.clientX, y: event.clientY };
		if (dragging) {
			extendTo(event.clientX, event.clientY);
			return;
		}
		if (cursorFrame) return;
		cursorFrame = requestAnimationFrame(() => {
			cursorFrame = 0;
			if (!scroller) return;
			const target = document.elementFromPoint(lastPointer.x, lastPointer.y);
			if (!target?.closest('.page')) {
				overText = false;
				return;
			}
			const page = pageUnder(lastPointer.y);
			const text = tab.text(page);
			const [x, y] = toPage(page, lastPointer.x, lastPointer.y);
			overText = !!text && isOverText(text, x, y);
		});
	}

	function onPointerUp(event: PointerEvent) {
		if (!dragging) return;
		extendTo(event.clientX, event.clientY);
		dragging = null;
		cancelAnimationFrame(autoScroll);
		if (scroller?.hasPointerCapture(event.pointerId)) scroller.releasePointerCapture(event.pointerId);
		const sel = tab.selection;
		if (sel && sel.anchor.page === sel.focus.page && sel.anchor.line === sel.focus.line && sel.anchor.offset === sel.focus.offset) {
			tab.selection = null;
		}
	}

	/** Scrolls while a selection is dragged past the top or bottom edge. */
	function autoScrollStep() {
		if (!dragging || !scroller) return;
		const rect = scroller.getBoundingClientRect();
		const edge = 32;
		let dy = 0;
		if (lastPointer.y < rect.top + edge) dy = -Math.min(40, rect.top + edge - lastPointer.y);
		else if (lastPointer.y > rect.bottom - edge) dy = Math.min(40, lastPointer.y - (rect.bottom - edge));
		if (dy !== 0) {
			scroller.scrollTop += dy;
			onScroll();
			extendTo(lastPointer.x, lastPointer.y);
		}
		autoScroll = requestAnimationFrame(autoScrollStep);
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
					class:cursor-text={overText}
					tabindex="0"
					role="document"
					aria-label="{tab.name}, {tab.pageCount} pages"
					onscroll={onScroll}
					onpointerdown={chain(props, 'onpointerdown', onPointerDown)}
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
