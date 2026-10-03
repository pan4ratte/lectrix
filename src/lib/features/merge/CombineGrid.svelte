<script lang="ts">
	// Every page of the result in one grid (section 6.4). Virtualized by rows: two
	// 500-page files are 1,000 thumbnails. Click selects, Ctrl+click adds, Shift+click
	// extends; dragging moves the selection; arrows move the focus (Shift extends);
	// Alt+Shift+arrows move the selected pages; R and Shift+R turn them; Delete removes them.
	import { RotateCcw, RotateCw, X } from '@lucide/svelte';
	import { onMount, tick } from 'svelte';

	import { pageUrl } from '#lib/ipc/index.ts';
	import RenderedImage from '#lib/features/viewer/RenderedImage.svelte';

	import type { CombineState } from './combine.svelte.ts';
	import { gapAt, gridFor, type CombinePage, type Gap } from './pages.ts';

	let { combine }: { combine: CombineState } = $props();

	/** The square each thumbnail is fitted into, and the cell around it, in CSS pixels. */
	const THUMB = 120;
	const CELL_W = 136;
	const CELL_H = THUMB + 52;
	const GAP = 8;
	const PAD = 16;
	const DRAG_THRESHOLD = 6;

	let scroller: HTMLDivElement | undefined = $state();
	let width = $state(0);
	let viewportH = $state(0);
	let scrollTop = $state(0);
	let dpr = $state(typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1);

	const pages = $derived(combine.pages);
	const grid = $derived(gridFor(width, CELL_W, CELL_H, GAP, PAD));
	const rowCount = $derived(Math.ceil(pages.length / grid.columns));
	const totalHeight = $derived(rowCount ? 2 * PAD + rowCount * grid.pitchY - GAP : 0);
	const focusIndex = $derived(Math.max(0, pages.findIndex((p) => p.key === combine.focusKey)));

	/** Indexes of the cells in and near the viewport. */
	const visible = $derived.by(() => {
		if (!pages.length) return [] as number[];
		const firstRow = Math.max(0, Math.floor((scrollTop - PAD) / grid.pitchY) - 1);
		const lastRow = Math.min(rowCount - 1, Math.ceil((scrollTop + viewportH) / grid.pitchY) + 1);
		const out: number[] = [];
		for (let i = firstRow * grid.columns; i < Math.min(pages.length, (lastRow + 1) * grid.columns); i++) out.push(i);
		return out;
	});
	const centerIndex = $derived(
		Math.floor((scrollTop + viewportH / 2 - PAD) / grid.pitchY) * grid.columns + Math.floor(grid.columns / 2)
	);

	function cellPosition(index: number) {
		return {
			x: PAD + (index % grid.columns) * grid.pitchX,
			y: PAD + Math.floor(index / grid.columns) * grid.pitchY
		};
	}

	/** The thumbnail's size as shown (turned) and as rendered (the page's own orientation). */
	function thumbFor(page: CombinePage) {
		const size = combine.source(page.source)?.pages[page.page] ?? { width: 612, height: 792 };
		const turned = page.rotation === 90 || page.rotation === 270;
		const w = turned ? size.height : size.width;
		const h = turned ? size.width : size.height;
		const fit = Math.min(THUMB / w, THUMB / h);
		// Scales snap to 1/1000 so revisiting a page hits the image cache.
		const scale = Math.round(fit * dpr * 1000) / 1000;
		return {
			shownW: w * fit,
			shownH: h * fit,
			imageW: size.width * fit,
			imageH: size.height * fit,
			scale
		};
	}

	function labelOf(page: CombinePage): string {
		const label = combine.source(page.source)?.labels?.[page.page];
		const number = String(page.page + 1);
		return label && label !== number ? `${label} (${number})` : number;
	}

	function describe(page: CombinePage, index: number): string {
		const source = combine.source(page.source);
		const turned = page.rotation ? `, turned ${page.rotation}°` : '';
		return `${index + 1} of ${pages.length}: ${source?.name ?? ''}, page ${labelOf(page)}${turned}`;
	}

	function ensureVisible(index: number) {
		if (!scroller) return;
		const { y } = cellPosition(index);
		if (y < scroller.scrollTop + PAD) scroller.scrollTop = y - PAD;
		else if (y + CELL_H > scroller.scrollTop + viewportH - PAD) scroller.scrollTop = y + CELL_H - viewportH + PAD;
	}

	// ----- keyboard -----

	function onKeyDown(event: KeyboardEvent) {
		if (!pages.length) return;
		const ctrl = event.ctrlKey || event.metaKey;
		const selected = combine.selectedSet;
		let target: number | null = null;
		switch (event.key) {
			case 'ArrowLeft':
			case 'ArrowRight':
			case 'ArrowUp':
			case 'ArrowDown': {
				const back = event.key === 'ArrowLeft' || event.key === 'ArrowUp';
				if (event.altKey && event.shiftKey) {
					combine.nudge(back ? -1 : 1);
					event.preventDefault();
					void tick().then(() => ensureVisible(Math.max(0, pages.findIndex((p) => selected.has(p.key)))));
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
				target = pages.length - 1;
				break;
			case 'PageUp':
			case 'PageDown': {
				const rows = Math.max(1, Math.floor(viewportH / grid.pitchY));
				target = focusIndex + (event.key === 'PageUp' ? -1 : 1) * rows * grid.columns;
				break;
			}
			case ' ': {
				const key = pages[focusIndex]?.key;
				if (key !== undefined) combine.select(key, 'toggle');
				event.preventDefault();
				return;
			}
			case 'a':
			case 'A':
				if (!ctrl) return;
				combine.selectAll();
				event.preventDefault();
				return;
			case 'Delete':
			case 'Backspace':
				combine.remove(selected);
				event.preventDefault();
				void tick().then(() => ensureVisible(focusIndex));
				return;
			case 'r':
			case 'R':
				if (ctrl || event.altKey) return;
				combine.rotate(selected, event.shiftKey ? -90 : 90);
				event.preventDefault();
				return;
			case 'Escape':
				if (combine.selected.length) {
					combine.selected = [];
					event.preventDefault();
				}
				return;
			default:
				return;
		}
		event.preventDefault();
		const index = Math.max(0, Math.min(pages.length - 1, target));
		const key = pages[index]!.key;
		if (event.shiftKey) combine.select(key, 'range');
		else if (ctrl) combine.focusKey = key;
		else combine.select(key, 'single');
		ensureVisible(index);
	}

	// ----- pointer: select, drag to reorder -----

	let press: { key: number; x: number; y: number; reselect: boolean } | null = null;
	let drag = $state<{ keys: Set<number>; x: number; y: number; gap: Gap | null } | null>(null);
	let autoScroll = 0;
	let lastPointer = { x: 0, y: 0 };

	function onPointerDown(event: PointerEvent) {
		if (event.button !== 0 || !scroller) return;
		const target = event.target as HTMLElement;
		if (target.closest('[data-tool]')) return;
		scroller.focus({ preventScroll: true });
		const cell = target.closest<HTMLElement>('[data-key]');
		if (!cell) {
			if (!event.ctrlKey && !event.shiftKey) combine.selected = [];
			return;
		}
		event.preventDefault();
		const key = Number(cell.dataset.key);
		let reselect = false;
		if (event.shiftKey) combine.select(key, 'range');
		else if (event.ctrlKey || event.metaKey) combine.select(key, 'toggle');
		else if (!combine.selectedSet.has(key)) combine.select(key, 'single');
		// A plain click on a selected page selects only it, unless it starts a drag.
		else reselect = true;
		press = { key, x: event.clientX, y: event.clientY, reselect };
		scroller.setPointerCapture(event.pointerId);
	}

	function onPointerMove(event: PointerEvent) {
		lastPointer = { x: event.clientX, y: event.clientY };
		if (!press) return;
		if (!drag) {
			if (Math.hypot(event.clientX - press.x, event.clientY - press.y) < DRAG_THRESHOLD) return;
			if (!combine.selectedSet.has(press.key)) combine.select(press.key, 'single');
			drag = { keys: new Set(combine.selected), x: event.clientX, y: event.clientY, gap: null };
			autoScroll = requestAnimationFrame(autoScrollStep);
		}
		updateDrop(event.clientX, event.clientY);
	}

	function updateDrop(clientX: number, clientY: number) {
		if (!drag || !scroller) return;
		const rect = scroller.getBoundingClientRect();
		drag.x = clientX;
		drag.y = clientY;
		drag.gap = gapAt(grid, pages.length, clientX - rect.left, clientY - rect.top + scroller.scrollTop);
	}

	/** Scrolls while pages are dragged near the top or bottom edge. */
	function autoScrollStep() {
		if (!drag || !scroller) return;
		const rect = scroller.getBoundingClientRect();
		const edge = 40;
		let dy = 0;
		if (lastPointer.y < rect.top + edge) dy = -Math.min(20, rect.top + edge - lastPointer.y);
		else if (lastPointer.y > rect.bottom - edge) dy = Math.min(20, lastPointer.y - (rect.bottom - edge));
		if (dy !== 0) {
			scroller.scrollTop += dy;
			updateDrop(lastPointer.x, lastPointer.y);
		}
		autoScroll = requestAnimationFrame(autoScrollStep);
	}

	function onPointerUp(event: PointerEvent) {
		if (scroller?.hasPointerCapture(event.pointerId)) scroller.releasePointerCapture(event.pointerId);
		cancelAnimationFrame(autoScroll);
		const pressed = press;
		const dragged = drag;
		press = null;
		drag = null;
		if (dragged?.gap) combine.move(dragged.keys, dragged.gap.gap);
		else if (pressed?.reselect) combine.select(pressed.key, 'single');
	}

	function onPointerCancel() {
		cancelAnimationFrame(autoScroll);
		press = null;
		drag = null;
	}

	onMount(() => {
		if (!scroller) return;
		const resize = new ResizeObserver(() => {
			width = scroller?.clientWidth ?? 0;
			viewportH = scroller?.clientHeight ?? 0;
			dpr = window.devicePixelRatio || 1;
		});
		resize.observe(scroller);
		return () => {
			resize.disconnect();
			cancelAnimationFrame(autoScroll);
		};
	});
</script>

<div
	bind:this={scroller}
	class="combine-grid relative min-h-0 flex-1 overflow-y-auto bg-canvas"
	role="listbox"
	aria-label="Pages of the combined file"
	aria-multiselectable="true"
	aria-activedescendant={combine.focusKey !== null ? `combine-page-${combine.focusKey}` : undefined}
	tabindex="0"
	onscroll={() => (scrollTop = scroller?.scrollTop ?? 0)}
	onkeydown={onKeyDown}
	onpointerdown={onPointerDown}
	onpointermove={onPointerMove}
	onpointerup={onPointerUp}
	onpointercancel={onPointerCancel}
	onlostpointercapture={onPointerCancel}
>
	<div class="relative" style:height="{totalHeight}px">
		{#each visible as index (pages[index]!.key)}
			{@const page = pages[index]!}
			{@const pos = cellPosition(index)}
			{@const thumb = thumbFor(page)}
			{@const source = combine.source(page.source)}
			{@const selected = combine.selectedSet.has(page.key)}
			<div
				id="combine-page-{page.key}"
				data-key={page.key}
				class="combine-cell group absolute flex flex-col items-center rounded-panel pt-2"
				class:combine-cell-selected={selected}
				class:combine-cell-focus={page.key === combine.focusKey}
				class:opacity-40={drag?.keys.has(page.key)}
				style:left="{pos.x}px"
				style:top="{pos.y}px"
				style:width="{CELL_W}px"
				style:height="{CELL_H}px"
				role="option"
				aria-selected={selected}
				aria-label={describe(page, index)}
				aria-posinset={index + 1}
				aria-setsize={pages.length}
			>
				<span class="relative flex items-center justify-center" style:width="{THUMB}px" style:height="{THUMB}px">
					<span
						class="combine-thumb relative block bg-white"
						style:width="{thumb.shownW}px"
						style:height="{thumb.shownH}px"
					>
						<span
							class="absolute top-1/2 left-1/2 block"
							style:width="{thumb.imageW}px"
							style:height="{thumb.imageH}px"
							style:transform="translate(-50%, -50%) rotate({page.rotation}deg)"
						>
							{#if source}
								<RenderedImage
									imageKey="{source.id}:{page.page}:{source.revision}:{thumb.scale}"
									url={pageUrl(source.id, page.page, thumb.scale, source.revision)}
									priority={500_000 + Math.abs(index - centerIndex)}
									x={0}
									y={0}
									width={thumb.imageW}
									height={thumb.imageH}
								/>
							{/if}
						</span>
					</span>
					<span
						class="absolute top-0 right-0 hidden gap-0.5 rounded-control bg-surface-raised p-0.5 shadow-[0_1px_4px_var(--color-page-shadow)] group-hover:flex"
					>
						<button
							type="button"
							data-tool
							class="icon-button size-6"
							tabindex="-1"
							aria-label="Turn counter-clockwise"
							title="Turn counter-clockwise (Shift+R)"
							onclick={() => combine.rotate(new Set([page.key]), -90)}
						>
							<RotateCcw size={14} aria-hidden="true" />
						</button>
						<button
							type="button"
							data-tool
							class="icon-button size-6"
							tabindex="-1"
							aria-label="Turn clockwise"
							title="Turn clockwise (R)"
							onclick={() => combine.rotate(new Set([page.key]), 90)}
						>
							<RotateCw size={14} aria-hidden="true" />
						</button>
						<button
							type="button"
							data-tool
							class="icon-button size-6"
							tabindex="-1"
							aria-label="Remove"
							title="Remove (Del)"
							onclick={() => combine.remove(new Set([page.key]))}
						>
							<X size={14} aria-hidden="true" />
						</button>
					</span>
				</span>
				<span class="mt-1.5 max-w-full truncate px-1 text-xs tabular-nums">{labelOf(page)}</span>
				<span class="flex max-w-full items-center gap-1 px-1 text-xs text-fg-muted">
					<span
						class="size-2 shrink-0 rounded-full"
						style:background="var(--folio-source-{source?.color ?? 1})"
						aria-hidden="true"
					></span>
					<span class="truncate">{source?.name ?? ''}</span>
				</span>
			</div>
		{/each}
		{#if drag?.gap}
			<div
				class="combine-drop-line"
				style:left="{drag.gap.x - GAP / 2 - 1.5}px"
				style:top="{drag.gap.y}px"
				style:height="{CELL_H}px"
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
