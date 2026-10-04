<script lang="ts">
	// Thumbnails panel: a virtualized list of small page images with their labels. Click
	// (or Enter) jumps to the page; arrow keys move between thumbnails.
	import { ContextMenu } from 'bits-ui';
	import { onMount, tick } from 'svelte';

	import { chain } from '#lib/components/chain.ts';
	import { startRangeAt } from '#lib/features/labels/actions.ts';
	import { pageUrl } from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { pageBoxText } from './pagebox.ts';
	import RenderedImage from './RenderedImage.svelte';
	import { BACKGROUND_PRIORITY } from './scheduler.ts';

	let { tab }: { tab: DocTab } = $props();

	const THUMB_WIDTH = 112;
	const LABEL_HEIGHT = 22;
	const GAP = 12;
	const PAD = 12;

	let list: HTMLDivElement | undefined = $state();
	let scrollTop = $state(0);
	let viewportH = $state(0);
	let dpr = $state(typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1);
	let focused = $state(0);

	/** Thumbnail height for each page (the page's aspect, document rotation applied). */
	const items = $derived.by(() => {
		let top = PAD;
		return tab.pages.map((p) => {
			const imageH = Math.round((THUMB_WIDTH * p.height) / p.width);
			const item = { top, imageH, height: imageH + LABEL_HEIGHT };
			top += item.height + GAP;
			return item;
		});
	});
	const totalHeight = $derived(items.length ? items[items.length - 1]!.top + items[items.length - 1]!.height + PAD : 0);

	function indexAt(y: number) {
		let lo = 0;
		let hi = items.length - 1;
		while (lo < hi) {
			const mid = (lo + hi + 1) >> 1;
			if (items[mid]!.top <= y) lo = mid;
			else hi = mid - 1;
		}
		return lo;
	}

	const range = $derived.by(() => {
		if (!items.length) return [] as number[];
		const first = indexAt(Math.max(0, scrollTop - viewportH / 2));
		const last = indexAt(scrollTop + viewportH * 1.5);
		const out: number[] = [];
		for (let i = first; i <= last; i++) out.push(i);
		return out;
	});

	function scale(index: number) {
		const p = tab.pages[index]!;
		// Thumbnail scales snap to 1/1000 so repeated visits hit the image cache.
		return Math.round(((THUMB_WIDTH * dpr) / p.width) * 1000) / 1000;
	}

	function ensureVisible(index: number) {
		const item = items[index];
		if (!list || !item) return;
		if (item.top < list.scrollTop || item.top + item.height > list.scrollTop + viewportH) {
			list.scrollTop = item.top - viewportH / 2 + item.height / 2;
		}
	}

	// Follow the current page as the document scrolls.
	$effect(() => {
		const current = tab.currentPage;
		focused = current;
		ensureVisible(current);
	});

	function go(index: number) {
		tab.viewer?.goTo({ page: index, offset: 0 });
	}

	async function onKeyDown(event: KeyboardEvent) {
		let next = focused;
		if (event.key === 'ArrowDown') next = Math.min(tab.pageCount - 1, focused + 1);
		else if (event.key === 'ArrowUp') next = Math.max(0, focused - 1);
		else if (event.key === 'Home') next = 0;
		else if (event.key === 'End') next = tab.pageCount - 1;
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
			dpr = window.devicePixelRatio || 1;
		});
		resize.observe(list);
		return () => resize.disconnect();
	});
</script>

<ContextMenu.Root>
	<ContextMenu.Trigger>
		{#snippet child({ props })}
			<div
				{...props}
				bind:this={list}
				class="h-full overflow-y-auto"
				onscroll={() => (scrollTop = list?.scrollTop ?? 0)}
				role="listbox"
				aria-label="Page thumbnails"
				tabindex="-1"
				onkeydown={chain(props, 'onkeydown', onKeyDown)}
			>
				<div class="relative" style:height="{totalHeight}px">
					{#each range as index (index)}
						{@const item = items[index]!}
						{@const current = index === tab.currentPage}
						<button
							type="button"
							data-thumb={index}
							class="thumb absolute left-1/2 flex -translate-x-1/2 flex-col items-center gap-1 rounded-control p-1 outline-offset-1"
							class:thumb-current={current}
							style:top="{item.top}px"
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
								style:width="{THUMB_WIDTH}px"
								style:height="{item.imageH}px"
							>
								<RenderedImage
									imageKey="{tab.id}:{index}:{tab.state.revision}:{scale(index)}"
									url={pageUrl(tab.id, index, scale(index), tab.state.revision)}
									priority={BACKGROUND_PRIORITY + Math.abs(index - tab.currentPage)}
									x={0}
									y={0}
									width={THUMB_WIDTH}
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
