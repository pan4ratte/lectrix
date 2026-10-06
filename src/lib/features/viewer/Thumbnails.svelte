<script lang="ts">
	// Pages panel: a toolbar, then a virtualized list of small page images with their labels,
	// in one column or in a grid. Click (or Enter) jumps to the page; arrow keys move between
	// thumbnails. The toolbar turns the current page (in the document, undoably), makes
	// thumbnails smaller or larger, fits them to the panel's width (following it as it
	// changes), and switches between one column and a grid of as many columns as fit. A new
	// size glides there in 140 ms, as zooming the view does.
	import { LayoutGrid, MoveHorizontal, RotateCcw, RotateCw, SquareText, ZoomIn, ZoomOut } from '@lucide/svelte';
	import { ContextMenu } from 'bits-ui';
	import { onMount, tick, untrack } from 'svelte';

	import { chain } from '#lib/components/chain.ts';
	import { motionMs } from '#lib/components/panes.ts';
	import { startRangeAt } from '#lib/features/labels/actions.ts';
	import { pageUrl } from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { pageBoxText } from './pagebox.ts';
	import RenderedImage from './RenderedImage.svelte';
	import { BACKGROUND_PRIORITY } from './scheduler.ts';
	import { layoutThumbs, renderWidth, shownWidth, stepWidth } from './thumbs.ts';
	import { easeOut, ZOOM_GLIDE_MS } from './zoom.ts';

	let { tab }: { tab: DocTab } = $props();

	let list: HTMLDivElement | undefined = $state();
	let scrollTop = $state(0);
	let viewportH = $state(0);
	let listW = $state(0);
	let dpr = $state(typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1);
	let focused = $state(0);

	/** Thumbnail width: the panel's when fitting it, else the chosen one (at most the panel's). */
	const targetW = $derived(shownWidth(app.thumbnailWidth, app.thumbnailsFit, listW));
	const smaller = $derived(stepWidth(targetW, -1, listW));
	const larger = $derived(stepWidth(targetW, 1, listW));
	/** The width laid out now: on its way to `targetW` during a glide. */
	let thumbW = $state(untrack(() => targetW));

	/** The next width change comes from a button, so it glides. */
	let glideNext = false;

	/** Smaller or Larger: a chosen width, no longer the panel's. */
	function resize(width: number | null) {
		if (width === null) return;
		glideNext = true;
		app.thumbnailWidth = width;
		app.thumbnailsFit = false;
	}

	function toggleFit() {
		glideNext = true;
		app.thumbnailsFit = !app.thumbnailsFit;
	}

	const layout = $derived(layoutThumbs(tab.pages, thumbW, listW, app.thumbnailsGrid));
	const items = $derived(layout.items);

	/** The row at `y` in the list (the last one starting above it). */
	function rowAt(y: number) {
		const rows = layout.rows;
		let lo = 0;
		let hi = rows.length - 1;
		while (lo < hi) {
			const mid = (lo + hi + 1) >> 1;
			if (rows[mid]!.top <= y) lo = mid;
			else hi = mid - 1;
		}
		return lo;
	}

	const range = $derived.by(() => {
		const rows = layout.rows;
		if (!rows.length) return [] as number[];
		const first = rows[rowAt(Math.max(0, scrollTop - viewportH / 2))]!.first;
		const last = rows[rowAt(scrollTop + viewportH * 1.5)]!.last;
		const out: number[] = [];
		for (let i = first; i <= last; i++) out.push(i);
		return out;
	});

	function scale(index: number) {
		const p = tab.pages[index]!;
		// Rendered once, for the width a glide ends at. Scales snap to 1/1000 so repeated
		// visits hit the image cache.
		return Math.round(((renderWidth(targetW) * dpr) / p.width) * 1000) / 1000;
	}

	function center(index: number) {
		const item = items[index];
		if (list && item) list.scrollTop = item.top - viewportH / 2 + item.height / 2;
	}

	function ensureVisible(index: number) {
		const item = items[index];
		if (!list || !item) return;
		if (item.top < list.scrollTop || item.top + item.height > list.scrollTop + viewportH) center(index);
	}

	// A new width glides there (from a button, unless Settings' smooth zooming is off or the
	// system asks for reduced motion) or snaps (the panel resized while fitting it), keeping
	// the current page's thumbnail in the middle of the list.
	let glide = 0;
	$effect(() => {
		const to = targetW;
		untrack(() => {
			const from = thumbW;
			const animate = glideNext;
			glideNext = false;
			if (to === from) return;
			cancelAnimationFrame(glide);
			const duration = animate && app.settings?.smoothZoom !== false ? motionMs(ZOOM_GLIDE_MS) : 0;
			const keep = () => void tick().then(() => center(tab.currentPage));
			if (duration === 0) {
				thumbW = to;
				keep();
				return;
			}
			const start = performance.now();
			const frame = (now: number) => {
				const t = Math.min(1, (now - start) / duration);
				thumbW = from + (to - from) * easeOut(t);
				keep();
				if (t < 1) glide = requestAnimationFrame(frame);
			};
			glide = requestAnimationFrame(frame);
		});
	});

	// A grid of another shape keeps the current page in the middle too.
	let lastColumns = 0;
	$effect(() => {
		const columns = layout.columns;
		if (lastColumns !== 0 && columns !== lastColumns) void tick().then(() => center(untrack(() => tab.currentPage)));
		lastColumns = columns;
	});

	// Follow the current page as the document scrolls.
	$effect(() => {
		const current = tab.currentPage;
		focused = current;
		untrack(() => ensureVisible(current));
	});

	function go(index: number) {
		tab.viewer?.goTo({ page: index, offset: 0 });
	}

	async function onKeyDown(event: KeyboardEvent) {
		const last = tab.pageCount - 1;
		const columns = layout.columns;
		let next = focused;
		if (event.key === 'ArrowDown') next = Math.min(last, focused + columns);
		else if (event.key === 'ArrowUp') next = Math.max(0, focused - columns);
		else if (event.key === 'ArrowRight' && columns > 1) next = Math.min(last, focused + 1);
		else if (event.key === 'ArrowLeft' && columns > 1) next = Math.max(0, focused - 1);
		else if (event.key === 'Home') next = 0;
		else if (event.key === 'End') next = last;
		else return;
		event.preventDefault();
		focused = next;
		ensureVisible(next);
		await tick();
		list?.querySelector<HTMLButtonElement>(`[data-thumb="${next}"]`)?.focus();
		go(next);
	}

	let contextPage = $state(0);

	onMount(() => {
		if (!list) return;
		const resize = new ResizeObserver(() => {
			viewportH = list?.clientHeight ?? 0;
			listW = list?.clientWidth ?? 0;
			dpr = window.devicePixelRatio || 1;
		});
		resize.observe(list);
		return () => {
			resize.disconnect();
			cancelAnimationFrame(glide);
		};
	});
</script>

<div class="flex h-full min-h-0 flex-col">
	<div class="flex shrink-0 flex-wrap items-center justify-center gap-0.5 border-b border-line px-2 py-[6px]">
		<h2 class="sr-only">Pages</h2>
		<button
			type="button"
			class="icon-button"
			aria-label="Rotate current page counter-clockwise"
			title="Rotate current page counter-clockwise"
			disabled={!tab.flags.canAssemble}
			onclick={() => void app.rotatePages(tab, [tab.currentPage], -90)}
		>
			<RotateCcw size={16} aria-hidden="true" />
		</button>
		<button
			type="button"
			class="icon-button"
			aria-label="Rotate current page clockwise"
			title="Rotate current page clockwise"
			disabled={!tab.flags.canAssemble}
			onclick={() => void app.rotatePages(tab, [tab.currentPage], 90)}
		>
			<RotateCw size={16} aria-hidden="true" />
		</button>
		<span class="mx-1 h-5 w-px shrink-0 bg-line" aria-hidden="true"></span>
		<button
			type="button"
			class="icon-button"
			aria-label="Smaller thumbnails"
			title="Smaller thumbnails"
			disabled={smaller === null}
			onclick={() => resize(smaller)}
		>
			<ZoomOut size={16} aria-hidden="true" />
		</button>
		<button
			type="button"
			class="icon-button"
			aria-label="Larger thumbnails"
			title="Larger thumbnails"
			disabled={larger === null}
			onclick={() => resize(larger)}
		>
			<ZoomIn size={16} aria-hidden="true" />
		</button>
		<button
			type="button"
			class="icon-button tool-button"
			aria-label="Fit thumbnails to the panel width"
			title="Fit thumbnails to the panel width"
			aria-pressed={app.thumbnailsFit}
			onclick={toggleFit}
		>
			<MoveHorizontal size={16} aria-hidden="true" />
		</button>
		<!-- One switch, its icon showing the layout in use. -->
		<button
			type="button"
			class="icon-button"
			aria-label="Thumbnails in a grid"
			title={app.thumbnailsGrid ? 'Grid: as many columns as fit (click for one column)' : 'One column (click for a grid)'}
			aria-pressed={app.thumbnailsGrid}
			onclick={() => (app.thumbnailsGrid = !app.thumbnailsGrid)}
		>
			{#if app.thumbnailsGrid}
				<LayoutGrid size={16} aria-hidden="true" />
			{:else}
				<SquareText size={16} aria-hidden="true" />
			{/if}
		</button>
	</div>
	<ContextMenu.Root>
		<ContextMenu.Trigger>
			{#snippet child({ props })}
				<div
					{...props}
					bind:this={list}
					class="min-h-0 flex-1 overflow-y-auto"
					onscroll={() => (scrollTop = list?.scrollTop ?? 0)}
					role="listbox"
					aria-label="Page thumbnails"
					tabindex="-1"
					onkeydown={chain(props, 'onkeydown', onKeyDown)}
				>
					<div class="relative" style:height="{layout.height}px">
						{#each range as index (index)}
							{@const item = items[index]!}
							{@const current = index === tab.currentPage}
							<button
								type="button"
								data-thumb={index}
								class="thumb absolute flex flex-col items-center gap-1 rounded-control p-1 outline-offset-1"
								class:thumb-current={current}
								style:top="{item.top}px"
								style:left="{item.left}px"
								role="option"
								aria-selected={current}
								aria-label="Page {pageBoxText(index, tab.displayLabels)}"
								tabindex={index === focused ? 0 : -1}
								onclick={() => {
									focused = index;
									go(index);
								}}
								oncontextmenu={() => (contextPage = index)}
							>
								<span
									class="relative block bg-white shadow-[0_1px_2px_var(--color-page-shadow)]"
									style:width="{thumbW}px"
									style:height="{item.imageH}px"
								>
									<RenderedImage
										imageKey="{tab.id}:{index}:{tab.state.revision}:{scale(index)}"
										url={pageUrl(tab.id, index, scale(index), tab.state.revision)}
										priority={BACKGROUND_PRIORITY + Math.abs(index - tab.currentPage)}
										x={0}
										y={0}
										width={thumbW}
										height={item.imageH}
									/>
								</span>
								<span class="text-xs text-fg-muted tabular-nums">{pageBoxText(index, tab.displayLabels)}</span>
							</button>
						{/each}
					</div>
				</div>
			{/snippet}
		</ContextMenu.Trigger>
		<ContextMenu.Portal>
			<ContextMenu.Content class="menu-content">
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
				<ContextMenu.Item
					class="menu-item"
					disabled={!tab.flags.canAssemble}
					onSelect={() => void app.insertFromFile(tab, contextPage + 1)}
				>
					Insert pages from file after this page…
				</ContextMenu.Item>
				<ContextMenu.Separator class="menu-separator" />
				<ContextMenu.Item
					class="menu-item"
					disabled={!tab.canEditLabels}
					onSelect={() => void startRangeAt(tab, contextPage)}
				>
					New label range from this page
				</ContextMenu.Item>
			</ContextMenu.Content>
		</ContextMenu.Portal>
	</ContextMenu.Root>
</div>
