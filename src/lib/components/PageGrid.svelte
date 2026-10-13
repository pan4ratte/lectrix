<script lang="ts" generics="T extends { key: number }">
	// Pages as thumbnails in one grid, for the Combine view (section 6.4) and for organizing
	// a document's pages. Virtualized by rows: two 500-page files are 1,000 thumbnails.
	// Click selects, Ctrl+click adds, Shift+click extends; dragging a page's picture moves
	// the selection; dragging from anywhere else draws a rectangle that selects the pages
	// whose pictures it touches (with Ctrl or Shift, on top of what is selected). Arrows move
	// the focus (Shift extends); Alt+Shift+arrows move the selected pages; R and Shift+R turn
	// them; Delete removes them; Enter or a double-click opens a page, where that means
	// something. Ctrl+wheel and a touchpad pinch ask for larger or smaller pictures, where
	// their size can change, and the page under the pointer keeps its place meanwhile. What
	// the pages are, and what the edits do, is the model's.
	import { onMount, tick, untrack, type Snippet } from 'svelte';

	import RenderedImage from '#lib/features/viewer/RenderedImage.svelte';
	import { isWheelNotch, wheelZoomFactor } from '#lib/features/viewer/zoom.ts';

	import {
		boxBetween,
		boxesTouch,
		cellOrigin,
		cellsInRowsOf,
		fitPicture,
		gapAt,
		gridFor,
		withKeys,
		type Box,
		type Gap,
		type Grid,
		type GridPicture,
		type PageGridModel
	} from './pagegrid.ts';

	interface Props {
		/** The pages, in order. */
		items: T[];
		model: PageGridModel;
		/** How a page is drawn; null while that is not known (its picture stays empty). */
		picture: (item: T, index: number) => GridPicture | null;
		/** A page's accessible name. */
		describe: (item: T, index: number) => string;
		/** Names the list for screen readers. */
		label: string;
		/** Cells get the id `{idPrefix}-{key}`. */
		idPrefix: string;
		/** The lines under a page's picture. */
		caption: Snippet<[T, number]>;
		/** Buttons shown for the page under the pointer. */
		tools?: Snippet<[T, number]>;
		/** Where they show: at a corner of the picture, or over the caption, centered, in its
		 * place (the snippet then says what the caption says, before its buttons). */
		toolsAt?: 'corner' | 'caption';
		/** The square each picture is fitted into, in CSS pixels. */
		thumb?: number;
		/** The square the pictures are rendered for, when that is not `thumb`: the size a
		 * change of size ends at, so they are rendered once and not for every frame of it. */
		renderThumb?: number;
		/** Ctrl+wheel or a touchpad pinch asks for pictures `factor` times the size; `notch`
		 * is true for a mouse wheel notch (which glides) and false for a pinch (which follows
		 * the fingers). */
		onzoom?: (factor: number, notch: boolean) => void;
		/** The height the caption takes. */
		captionHeight?: number;
		/** False where the pages can only be selected and looked at. */
		editable?: boolean;
		/** Enter or a double-click on a page. */
		onopen?: (item: T, index: number) => void;
		/** A page to bring into the middle of the view: the grid does when this changes. */
		reveal?: { key: number } | null;
		/** Render priority of the page in the middle of the view; pages further off wait longer. */
		priority?: number;
	}

	let {
		items,
		model,
		picture,
		describe,
		label,
		idPrefix,
		caption,
		tools,
		toolsAt = 'corner',
		thumb = 120,
		renderThumb,
		onzoom,
		captionHeight = 52,
		editable = true,
		onopen,
		reveal = null,
		priority = 500_000
	}: Props = $props();

	const GAP = 8;
	const PAD = 16;
	/** Space above a picture's square inside its cell. */
	const CELL_TOP = 7;
	const DRAG_THRESHOLD = 6;

	const cellW = $derived(thumb + 16);
	const cellH = $derived(thumb + captionHeight);

	let scroller: HTMLDivElement | undefined = $state();
	let width = $state(0);
	let viewportH = $state(0);
	let scrollTop = $state(0);
	let dpr = $state(typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1);

	const grid = $derived(gridFor(width, cellW, cellH, GAP, PAD));
	const rowCount = $derived(Math.ceil(items.length / grid.columns));
	const totalHeight = $derived(rowCount ? 2 * PAD + rowCount * grid.pitchY - GAP : 0);
	const focusIndex = $derived(Math.max(0, items.findIndex((p) => p.key === model.focusKey)));

	/** Indexes of the cells in and near the viewport. */
	const visible = $derived.by(() => {
		if (!items.length) return [] as number[];
		const firstRow = Math.max(0, Math.floor((scrollTop - PAD) / grid.pitchY) - 1);
		const lastRow = Math.min(rowCount - 1, Math.ceil((scrollTop + viewportH) / grid.pitchY) + 1);
		const out: number[] = [];
		for (let i = firstRow * grid.columns; i < Math.min(items.length, (lastRow + 1) * grid.columns); i++) out.push(i);
		return out;
	});
	const centerIndex = $derived(
		Math.floor((scrollTop + viewportH / 2 - PAD) / grid.pitchY) * grid.columns + Math.floor(grid.columns / 2)
	);

	/** A page's picture: its size as shown (turned) and as rendered, and the image to ask for. */
	function thumbFor(item: T, index: number) {
		const p = picture(item, index);
		const fitted = fitPicture(p?.width ?? 612, p?.height ?? 792, p?.turn ?? 0, thumb);
		// Scales snap to 1/1000 so revisiting a page hits the image cache.
		const scale = Math.round(fitted.fit * ((renderThumb ?? thumb) / thumb) * dpr * 1000) / 1000;
		return { ...fitted, scale, picture: p };
	}

	/** Where a page's picture is in the grid's content. */
	function pictureBox(item: T, index: number): Box {
		const { x, y } = cellOrigin(grid, index);
		const { shownW, shownH } = thumbFor(item, index);
		const x0 = x + (cellW - shownW) / 2;
		const y0 = y + CELL_TOP + (thumb - shownH) / 2;
		return { x0, y0, x1: x0 + shownW, y1: y0 + shownH };
	}

	/** Notes on a page's cell which image its picture shows, for the tests and the
	 * measurements that wait for the grid to be drawn (`data-drawn`). */
	function markDrawn(key: number, imageKey: string) {
		document.getElementById(`${idPrefix}-${key}`)?.setAttribute('data-drawn', imageKey);
	}

	function ensureVisible(index: number) {
		if (!scroller) return;
		const { y } = cellOrigin(grid, index);
		if (y < scroller.scrollTop + PAD) scroller.scrollTop = y - PAD;
		else if (y + cellH > scroller.scrollTop + viewportH - PAD) scroller.scrollTop = y + cellH - viewportH + PAD;
	}

	// A page asked for comes into the middle of the view, once the grid knows its size.
	let revealed: object | null = null;
	$effect(() => {
		const target = reveal;
		if (!target || target === revealed || !width || !viewportH) return;
		revealed = target;
		const index = untrack(() => items.findIndex((p) => p.key === target.key));
		if (index < 0) return;
		// Once the content has its height.
		void tick().then(() => {
			if (scroller) scroller.scrollTop = cellOrigin(grid, index).y - (viewportH - cellH) / 2;
		});
	});

	// ----- the size of the pictures -----

	/** What keeps its place while the pictures change size: a page, how far down its cell
	 * (0 to 1), and where in the view that is. */
	interface ZoomAnchor {
		index: number;
		part: number;
		y: number;
		/** When it was last used: it holds for as long as the size goes on changing. */
		at: number;
	}
	const ANCHOR_MS = 300;
	let zoomAnchor: ZoomAnchor | null = null;
	/** The layout the scroll position belongs to, and the size it was laid out for. */
	let laid: { grid: Grid; cellH: number; thumb: number } | null = null;

	/** The page at a point of the view (from the grid's left and top), in layout `layout`. */
	function anchorAt(layout: { grid: Grid; cellH: number }, x: number, y: number): ZoomAnchor {
		const g = layout.grid;
		const inContent = (scroller?.scrollTop ?? 0) + y;
		const rows = Math.max(1, Math.ceil(items.length / g.columns));
		const row = Math.max(0, Math.min(rows - 1, Math.floor((inContent - g.pad) / g.pitchY)));
		const column = Math.max(0, Math.min(g.columns - 1, Math.floor((x - g.pad) / g.pitchX)));
		const index = Math.min(items.length - 1, row * g.columns + column);
		const part = Math.max(0, Math.min(1, (inContent - (g.pad + row * g.pitchY)) / layout.cellH));
		return { index, part, y, at: performance.now() };
	}

	// A new size keeps a page where it is: the one under the pointer for the wheel and a
	// pinch, else the one in the middle of the view. (After the content has its new height.)
	$effect(() => {
		const next = { grid, cellH, thumb };
		untrack(() => {
			const before = laid;
			laid = next;
			if (!scroller || !before || before.thumb === next.thumb || !items.length) return;
			const now = performance.now();
			if (!zoomAnchor || now - zoomAnchor.at > ANCHOR_MS) zoomAnchor = anchorAt(before, width / 2, viewportH / 2);
			zoomAnchor.at = now;
			scroller.scrollTop = cellOrigin(next.grid, zoomAnchor.index).y + zoomAnchor.part * next.cellH - zoomAnchor.y;
			scrollTop = scroller.scrollTop;
		});
	});

	function onWheel(event: WheelEvent) {
		if (!event.ctrlKey || !onzoom || !scroller) return;
		// (Ctrl+wheel would zoom the whole app; a pinch arrives as Ctrl+wheel.)
		event.preventDefault();
		const rect = scroller.getBoundingClientRect();
		zoomAnchor = anchorAt(laid ?? { grid, cellH }, event.clientX - rect.left, event.clientY - rect.top);
		onzoom(wheelZoomFactor(event.deltaY, event.deltaMode), isWheelNotch(event.deltaY, event.deltaMode));
	}

	/** On one of a page's buttons, which the grid leaves to themselves. */
	function onTool(target: Element): boolean {
		return target.closest('[data-tool] button') !== null;
	}

	// ----- keyboard -----

	function onKeyDown(event: KeyboardEvent) {
		if (!items.length) return;
		const ctrl = event.ctrlKey || event.metaKey;
		const selected = model.selectedSet;
		let target: number | null = null;
		switch (event.key) {
			case 'ArrowLeft':
			case 'ArrowRight':
			case 'ArrowUp':
			case 'ArrowDown': {
				const back = event.key === 'ArrowLeft' || event.key === 'ArrowUp';
				if (event.altKey && event.shiftKey) {
					if (editable) model.nudge(back ? -1 : 1);
					event.preventDefault();
					void tick().then(() => ensureVisible(Math.max(0, items.findIndex((p) => selected.has(p.key)))));
					return;
				}
				const step = event.key === 'ArrowUp' || event.key === 'ArrowDown' ? grid.columns : 1;
				target = focusIndex + (back ? -step : step);
				break;
			}
			case 'Home':
				target = 0;
				break;
			case 'End':
				target = items.length - 1;
				break;
			case 'PageUp':
			case 'PageDown': {
				const rows = Math.max(1, Math.floor(viewportH / grid.pitchY));
				target = focusIndex + (event.key === 'PageUp' ? -1 : 1) * rows * grid.columns;
				break;
			}
			case ' ': {
				const key = items[focusIndex]?.key;
				if (key !== undefined) model.select(key, 'toggle');
				event.preventDefault();
				return;
			}
			case 'Enter': {
				const item = items[focusIndex];
				if (!onopen || !item) return;
				onopen(item, focusIndex);
				event.preventDefault();
				return;
			}
			case 'a':
			case 'A':
				if (!ctrl) return;
				model.selectAll();
				event.preventDefault();
				return;
			case 'Delete':
			case 'Backspace':
				if (!editable) return;
				model.remove(selected);
				event.preventDefault();
				void tick().then(() => ensureVisible(focusIndex));
				return;
			case 'r':
			case 'R':
				if (ctrl || event.altKey || !editable) return;
				model.rotate(selected, event.shiftKey ? -90 : 90);
				event.preventDefault();
				return;
			case 'Escape':
				// A drag or a rectangle in progress ends first, leaving things as they were.
				if (drag || marquee) {
					if (marquee) model.setSelection([...marquee.before], null);
					stopPointer();
					event.preventDefault();
					event.stopPropagation();
				} else if (model.selected.length) {
					// Esc deselects here and does nothing more (leaving the tool, say).
					model.selected = [];
					event.preventDefault();
					event.stopPropagation();
				}
				return;
			default:
				return;
		}
		event.preventDefault();
		const index = Math.max(0, Math.min(items.length - 1, target));
		const key = items[index]!.key;
		if (event.shiftKey) model.select(key, 'range');
		else if (ctrl) model.focusKey = key;
		else model.select(key, 'single');
		ensureVisible(index);
	}

	// ----- pointer: select, drag to reorder, draw a rectangle -----

	interface Press {
		/** The page pressed, or null for the space between and around pages. */
		key: number | null;
		/** On the page's picture, which is what drags it. */
		onPicture: boolean;
		x: number;
		y: number;
		/** A plain press on a selected picture: selects only it, unless it starts a drag. */
		reselect: boolean;
		ctrl: boolean;
		shift: boolean;
	}

	let press: Press | null = null;
	let drag = $state<{ keys: Set<number>; x: number; y: number; gap: Gap | null } | null>(null);
	/** The rectangle being drawn: its start and end in the grid's content, the selection
	 * before it, and the part of that it adds to. */
	let marquee = $state<{ from: { x: number; y: number }; box: Box; before: number[]; base: number[] } | null>(null);
	let autoScroll = 0;
	let lastPointer = { x: 0, y: 0 };

	/** A point on screen, in the grid's content. */
	function inContent(clientX: number, clientY: number) {
		const rect = scroller!.getBoundingClientRect();
		return { x: clientX - rect.left, y: clientY - rect.top + scroller!.scrollTop };
	}

	function onPointerDown(event: PointerEvent) {
		if (event.button !== 0 || !scroller) return;
		const target = event.target as HTMLElement;
		if (onTool(target)) return;
		scroller.focus({ preventScroll: true });
		const cell = target.closest<HTMLElement>('[data-key]');
		const ctrl = event.ctrlKey || event.metaKey;
		const shift = event.shiftKey;
		const base = { x: event.clientX, y: event.clientY, ctrl, shift };
		if (!cell) {
			if (!ctrl && !shift) model.selected = [];
			press = { ...base, key: null, onPicture: false, reselect: false };
		} else {
			event.preventDefault();
			const key = Number(cell.dataset.key);
			const onPicture = target.closest('[data-picture]') !== null;
			let reselect = false;
			// A press on a picture selects at once, so that a drag moves what is selected.
			// Elsewhere in the cell it waits: the press may be the start of a rectangle.
			if (onPicture) {
				if (shift) model.select(key, 'range');
				else if (ctrl) model.select(key, 'toggle');
				else if (!model.selectedSet.has(key)) model.select(key, 'single');
				else reselect = true;
			}
			press = { ...base, key, onPicture, reselect };
		}
		scroller.setPointerCapture(event.pointerId);
	}

	function onPointerMove(event: PointerEvent) {
		lastPointer = { x: event.clientX, y: event.clientY };
		if (!press || !scroller) return;
		if (!drag && !marquee) {
			if (Math.hypot(event.clientX - press.x, event.clientY - press.y) < DRAG_THRESHOLD) return;
			if (press.key !== null && press.onPicture && editable) {
				if (!model.selectedSet.has(press.key)) model.select(press.key, 'single');
				drag = { keys: new Set(model.selected), x: event.clientX, y: event.clientY, gap: null };
			} else {
				const from = inContent(press.x, press.y);
				const before = [...model.selected];
				marquee = { from, box: boxBetween(from, from), before, base: press.ctrl || press.shift ? before : [] };
			}
			autoScroll = requestAnimationFrame(autoScrollStep);
		}
		if (drag) updateDrop(event.clientX, event.clientY);
		else updateMarquee(event.clientX, event.clientY);
	}

	function updateDrop(clientX: number, clientY: number) {
		if (!drag || !scroller) return;
		const at = inContent(clientX, clientY);
		drag.x = clientX;
		drag.y = clientY;
		drag.gap = gapAt(grid, items.length, at.x, at.y);
	}

	/** Stretches the rectangle to the pointer and selects the pages whose pictures it touches. */
	function updateMarquee(clientX: number, clientY: number) {
		if (!marquee || !scroller) return;
		const at = inContent(clientX, clientY);
		const box = boxBetween(marquee.from, at);
		marquee.box = box;
		const hits: number[] = [];
		let nearest: number | null = null;
		let nearestDistance = Infinity;
		const range = cellsInRowsOf(grid, items.length, box);
		for (let i = range?.first ?? 0; range && i <= range.last; i++) {
			const item = items[i]!;
			const shown = pictureBox(item, i);
			if (!boxesTouch(box, shown)) continue;
			hits.push(item.key);
			const distance = Math.hypot((shown.x0 + shown.x1) / 2 - at.x, (shown.y0 + shown.y1) / 2 - at.y);
			if (distance < nearestDistance) {
				nearestDistance = distance;
				nearest = item.key;
			}
		}
		model.setSelection(withKeys(marquee.base, hits), nearest);
	}

	/** Scrolls while pages are dragged, or a rectangle is drawn, near the top or bottom edge. */
	function autoScrollStep() {
		if ((!drag && !marquee) || !scroller) return;
		const rect = scroller.getBoundingClientRect();
		const edge = 40;
		let dy = 0;
		if (lastPointer.y < rect.top + edge) dy = -Math.min(20, rect.top + edge - lastPointer.y);
		else if (lastPointer.y > rect.bottom - edge) dy = Math.min(20, lastPointer.y - (rect.bottom - edge));
		if (dy !== 0) {
			scroller.scrollTop += dy;
			if (drag) updateDrop(lastPointer.x, lastPointer.y);
			else updateMarquee(lastPointer.x, lastPointer.y);
		}
		autoScroll = requestAnimationFrame(autoScrollStep);
	}

	function onPointerUp(event: PointerEvent) {
		if (scroller?.hasPointerCapture(event.pointerId)) scroller.releasePointerCapture(event.pointerId);
		const pressed = press;
		const dragged = drag;
		const drawn = marquee;
		stopPointer();
		if (dragged) {
			if (dragged.gap) model.move(dragged.keys, dragged.gap.gap);
		} else if (drawn || !pressed || pressed.key === null) {
			// A rectangle selected as it grew; a press on empty space cleared on the way down.
		} else if (pressed.onPicture) {
			if (pressed.reselect) model.select(pressed.key, 'single');
		} else {
			model.select(pressed.key, pressed.shift ? 'range' : pressed.ctrl ? 'toggle' : 'single');
		}
	}

	/** Ends a press, a drag or a rectangle without doing anything more. */
	function stopPointer() {
		cancelAnimationFrame(autoScroll);
		press = null;
		drag = null;
		marquee = null;
	}

	function onDoubleClick(event: MouseEvent) {
		// The grid holds the pointer while it is down, so the click's target is the grid:
		// what was clicked is what lies under the pointer.
		const target = document.elementFromPoint(event.clientX, event.clientY);
		if (!onopen || !target || !scroller?.contains(target) || onTool(target)) return;
		const cell = target.closest<HTMLElement>('[data-key]');
		const index = cell ? items.findIndex((p) => p.key === Number(cell.dataset.key)) : -1;
		if (index >= 0) onopen(items[index]!, index);
	}

	onMount(() => {
		if (!scroller) return;
		const resize = new ResizeObserver(() => {
			width = scroller?.clientWidth ?? 0;
			viewportH = scroller?.clientHeight ?? 0;
			dpr = window.devicePixelRatio || 1;
		});
		resize.observe(scroller);
		// Not passive: a Ctrl+wheel must not zoom the app.
		const element = scroller;
		element.addEventListener('wheel', onWheel, { passive: false });
		return () => {
			element.removeEventListener('wheel', onWheel);
			resize.disconnect();
			cancelAnimationFrame(autoScroll);
		};
	});
</script>

<div
	bind:this={scroller}
	class="page-grid relative min-h-0 flex-1 overflow-y-auto bg-canvas"
	role="listbox"
	aria-label={label}
	aria-multiselectable="true"
	aria-activedescendant={model.focusKey !== null ? `${idPrefix}-${model.focusKey}` : undefined}
	tabindex="0"
	onscroll={() => (scrollTop = scroller?.scrollTop ?? 0)}
	onkeydown={onKeyDown}
	onpointerdown={onPointerDown}
	onpointermove={onPointerMove}
	onpointerup={onPointerUp}
	onpointercancel={stopPointer}
	onlostpointercapture={stopPointer}
	ondblclick={onDoubleClick}
>
	<div class="relative" style:height="{totalHeight}px">
		{#each visible as index (items[index]!.key)}
			{@const item = items[index]!}
			{@const pos = cellOrigin(grid, index)}
			{@const shown = thumbFor(item, index)}
			{@const selected = model.selectedSet.has(item.key)}
			<div
				id="{idPrefix}-{item.key}"
				data-key={item.key}
				class="page-cell group absolute flex flex-col items-center rounded-panel hover:z-10"
				class:page-cell-selected={selected}
				class:page-cell-focus={item.key === model.focusKey}
				class:opacity-40={drag?.keys.has(item.key)}
				style:left="{pos.x}px"
				style:top="{pos.y}px"
				style:width="{cellW}px"
				style:height="{cellH}px"
				style:padding-top="{CELL_TOP}px"
				role="option"
				aria-selected={selected}
				aria-label={describe(item, index)}
				aria-posinset={index + 1}
				aria-setsize={items.length}
			>
				<span class="relative flex items-center justify-center" style:width="{thumb}px" style:height="{thumb}px">
					<span
						data-picture
						class="page-cell-picture relative block bg-white"
						style:width="{shown.shownW}px"
						style:height="{shown.shownH}px"
					>
						<span
							class="absolute top-1/2 left-1/2 block"
							style:width="{shown.imageW}px"
							style:height="{shown.imageH}px"
							style:transform="translate(-50%, -50%) rotate({shown.picture?.turn ?? 0}deg)"
						>
							{#if shown.picture}
								<RenderedImage
									imageKey={shown.picture.imageKey(shown.scale)}
									url={shown.picture.url(shown.scale)}
									priority={priority + Math.abs(index - centerIndex)}
									x={0}
									y={0}
									width={shown.imageW}
									height={shown.imageH}
									rotation={shown.picture.rotation ?? 0}
									ondrawn={(key) => markDrawn(item.key, key)}
								/>
							{/if}
						</span>
					</span>
					{#if tools && editable && toolsAt === 'corner'}
						<span
							data-tool
							class="absolute top-0 right-0 hidden gap-0.5 rounded-control bg-surface-raised p-0.5 shadow-[0_1px_4px_var(--color-page-shadow)] group-hover:flex"
						>
							{@render tools(item, index)}
						</span>
					{/if}
				</span>
				{@render caption(item, index)}
				{#if tools && editable && toolsAt === 'caption'}
					<span
						data-tool
						class="absolute bottom-0 left-1/2 hidden -translate-x-1/2 items-center gap-px rounded-control bg-surface-raised p-px shadow-[0_1px_4px_var(--color-page-shadow)] group-hover:flex"
					>
						{@render tools(item, index)}
					</span>
				{/if}
			</div>
		{/each}
		{#if drag?.gap}
			<div
				class="page-grid-drop-line"
				style:left="{drag.gap.x - GAP / 2 - 1.5}px"
				style:top="{drag.gap.y}px"
				style:height="{cellH}px"
				aria-hidden="true"
			></div>
		{/if}
		{#if marquee}
			<div
				class="page-grid-marquee"
				style:left="{marquee.box.x0}px"
				style:top="{marquee.box.y0}px"
				style:width="{marquee.box.x1 - marquee.box.x0}px"
				style:height="{marquee.box.y1 - marquee.box.y0}px"
				aria-hidden="true"
			></div>
		{/if}
	</div>
</div>

{#if drag}
	<div class="drag-ghost" style:left="{drag.x + 14}px" style:top="{drag.y + 14}px" aria-hidden="true">
		{drag.keys.size === 1 ? '1 page' : `${drag.keys.size} pages`}
	</div>
{/if}
