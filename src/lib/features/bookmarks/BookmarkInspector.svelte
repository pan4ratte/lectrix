<script lang="ts">
	// Inspector for the selected bookmark (section 8: shown only while something is
	// selected). Opened with "Properties" from the bookmark's context menu; it then follows
	// the selection until closed. It floats over the page canvas beside the bookmark's row in
	// the Bookmarks panel, with a tail pointing at the row, so opening it never changes the
	// zoom of a fit-width view. Without a row to stand beside (the panel hidden, the bookmark
	// inside a collapsed one), it stays at the top right of the canvas.
	import { Copy, Crosshair, ExternalLink, Trash, X } from '@lucide/svelte';
	import { untrack } from 'svelte';

	import ColorPicker from '#lib/features/annotations/ColorPicker.svelte';
	import { tailPath, tailShape, type Area } from '#lib/features/annotations/bars.ts';
	import { isWebAddress } from '#lib/features/viewer/links.ts';
	import { stopUnlessShortcut } from '#lib/shortcuts.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { copyLink, deleteBookmark, goToBookmark, renameBookmark, setBookmarkColor, setDestinationHere } from './actions.ts';
	import { describeTarget, descendantCount, locate } from './tree.ts';

	let { tab }: { tab: DocTab } = $props();

	/** The gap between the panel and the canvas's edge beside the pane, which the tail crosses. */
	const GAP = 16;
	/** How far above the row's middle the panel starts, so the tail sits near its top. */
	const TAIL_FROM_TOP = 28;
	/** The panel's corner radius (--radius-panel), which the tail keeps clear of. */
	const RADIUS = 8;

	const bookmark = $derived(
		tab.selectedBookmark !== null ? (locate(tab.outline.items, tab.selectedBookmark)?.bookmark ?? null) : null
	);
	const editable = $derived(tab.canEditBookmarks);
	const inside = $derived(bookmark ? descendantCount(bookmark) : 0);
	const shown = $derived(app.inspectorOpen && bookmark !== null && !tab.selectedAnnotation);

	let panel: HTMLElement | undefined = $state();
	let w = $state(0);
	let h = $state(0);
	/** Where the panel is beside the row, in the canvas's coordinates, and the row there
	 * (null while it is out of the tree's view); null without a row to stand beside. */
	let place = $state<{ left: number; top: number; row: Area | null } | null>(null);

	const top = $derived(app.overlayTop + (tab.search.open ? 52 : 0));

	function measure() {
		const host = panel?.offsetParent;
		const id = tab.selectedBookmark;
		const row = id !== null ? document.getElementById(`bookmark-${id}`) : null;
		const tree = row?.closest('[role=tree]');
		if (!panel || !host || !row || !tree || w === 0) {
			if (untrack(() => place) !== null) place = null;
			return;
		}
		const box = host.getBoundingClientRect();
		const r = row.getBoundingClientRect();
		const t = tree.getBoundingClientRect();
		let left: number;
		if (r.right <= box.left + 1) left = GAP;
		else if (r.left >= box.right - 1) left = box.width - w - Math.max(GAP, app.overlayRight);
		else {
			if (untrack(() => place) !== null) place = null;
			return;
		}
		// The row's middle, kept within the part of the tree in view.
		const middle = Math.max(t.top, Math.min((r.top + r.bottom) / 2, t.bottom)) - box.top;
		const lowest = Math.max(top, box.height - h - 12);
		const at = Math.max(top, Math.min(middle - TAIL_FROM_TOP, lowest));
		const inView = r.bottom > t.top && r.top < t.bottom;
		const next = {
			left,
			top: at,
			row: inView
				? { x0: r.left - box.left - left, y0: r.top - box.top - at, x1: r.right - box.left - left, y1: r.bottom - box.top - at }
				: null
		};
		if (JSON.stringify(next) !== JSON.stringify(untrack(() => place))) place = next;
	}

	// Stays beside the row as the tree scrolls, the panes or the window change size, and
	// panels move between the panes.
	$effect(() => {
		if (!shown || !panel) return;
		void tab.selectedBookmark;
		void tab.outline;
		void w;
		void h;
		void top;
		let frame = 0;
		const soon = () => {
			cancelAnimationFrame(frame);
			frame = requestAnimationFrame(measure);
		};
		measure();
		const resize = new ResizeObserver(soon);
		const host = panel.offsetParent;
		if (host) resize.observe(host);
		resize.observe(document.body);
		window.addEventListener('scroll', soon, true);
		window.addEventListener('resize', soon);
		window.addEventListener('transitionend', soon, true);
		window.addEventListener('animationend', soon, true);
		return () => {
			cancelAnimationFrame(frame);
			resize.disconnect();
			window.removeEventListener('scroll', soon, true);
			window.removeEventListener('resize', soon);
			window.removeEventListener('transitionend', soon, true);
			window.removeEventListener('animationend', soon, true);
		};
	});

	const tail = $derived(place?.row && w > 0 ? tailShape(w, h, place.row, RADIUS) : null);
	const tailSides = $derived(tail ? tailPath(tail) : '');

	/** Back to the usual text colour; the focus goes to the colour button, since this button
	 * goes with the colour. */
	async function clearColor(id: number) {
		await setBookmarkColor(tab, id, null);
		panel?.querySelector<HTMLElement>('[data-color-button]')?.focus();
	}

	function commit(event: Event) {
		if (!bookmark) return;
		const input = event.currentTarget as HTMLInputElement;
		void renameBookmark(tab, bookmark.id, input.value);
	}

	function onTitleKey(event: KeyboardEvent) {
		stopUnlessShortcut(event);
		if (event.key === 'Enter') {
			event.preventDefault();
			commit(event);
		} else if (event.key === 'Escape') {
			event.preventDefault();
			(event.currentTarget as HTMLInputElement).value = bookmark?.title ?? '';
		}
	}
</script>

{#if shown && bookmark}
	{@const b = bookmark}
	<div
		bind:this={panel}
		bind:offsetWidth={w}
		bind:offsetHeight={h}
		class="absolute z-10 w-72"
		style:top="{place?.top ?? top}px"
		style:left={place ? `${place.left}px` : null}
		style:right={place ? null : `${app.overlayRight}px`}
	>
		{#if tail}
			<svg class="comment-tail" width={w} height={h} aria-hidden="true">
				<path class="comment-tail-fill" d="{tailSides} Z" />
				<path class="comment-tail-edge" d={tailSides} />
			</svg>
		{/if}
		<aside
			class="flex flex-col gap-3 rounded-panel border border-line bg-surface-raised p-3 shadow-[0_4px_12px_var(--color-page-shadow)]"
			aria-label="Bookmark properties"
		>
			<div class="flex items-center">
				<h2 class="flex-1 text-sm font-semibold">Bookmark</h2>
				<button
					type="button"
					class="icon-button"
					aria-label="Close properties"
					onclick={() => (app.inspectorOpen = false)}
				>
					<X size={16} aria-hidden="true" />
				</button>
			</div>
			<!-- The field inherits font and color (app.css), so the label's small muted style
			     stays on its own span. -->
			<label class="flex flex-col gap-1">
				<span class="text-xs text-fg-muted">Title</span>
				{#key b.id}
					<input
						class="field h-8"
						value={b.title}
						readonly={!editable}
						onkeydown={onTitleKey}
						onblur={commit}
					/>
				{/key}
			</label>
			<div class="flex flex-col gap-1">
				<span class="text-xs text-fg-muted">Colour</span>
				<div class="flex items-center gap-2">
					{#if editable}
						<ColorPicker
							color={b.color}
							opacity={null}
							label="Bookmark colour"
							oncolor={(c) => void setBookmarkColor(tab, b.id, c)}
						/>
					{:else}
						<span class="swatch-dot" style:--swatch={b.color ?? 'transparent'} aria-hidden="true"></span>
					{/if}
					{#if b.color}
						<button
							type="button"
							class="button"
							disabled={!editable}
							onclick={() => void clearColor(b.id)}
							data-tip="Show the title in the usual text colour"
						>
							Default colour
						</button>
					{/if}
				</div>
			</div>
			<div class="flex flex-col gap-1">
				<span class="text-xs text-fg-muted">Destination</span>
				<p class="text-sm break-words select-text">{describeTarget(b.target, tab.labels)}</p>
				{#if inside > 0}
					<p class="text-xs text-fg-muted">Contains {inside === 1 ? '1 bookmark' : `${inside} bookmarks`}.</p>
				{/if}
			</div>
			<div class="flex flex-wrap gap-2">
				{#if b.target.kind === 'uri'}
					{@const uri = b.target.uri}
					{#if isWebAddress(uri)}
						<button type="button" class="button gap-1" onclick={() => goToBookmark(tab, b.id)}>
							<ExternalLink size={14} aria-hidden="true" />Open in browser
						</button>
					{/if}
					<button type="button" class="button gap-1" onclick={() => void copyLink(uri)}>
						<Copy size={14} aria-hidden="true" />Copy link
					</button>
				{/if}
				<button
					type="button"
					class="button gap-1"
					disabled={!editable}
					onclick={() => void setDestinationHere(tab, b.id)}
					data-tip="Point this bookmark at what is shown now"
				>
					<Crosshair size={14} aria-hidden="true" />Set to current view
				</button>
				<button
					type="button"
					class="button gap-1"
					disabled={!editable}
					onclick={() => void deleteBookmark(tab, b.id)}
				>
					<Trash size={14} aria-hidden="true" />Delete
				</button>
			</div>
		</aside>
	</div>
{/if}
