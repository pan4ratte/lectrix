<script lang="ts">
	// Documents side by side in one window (section 6.1, split view): the panes of a joined
	// tab in columns, each with a slim header (its document's name, and a button that gives it
	// a tab of its own again), its banners and its pages. Dividers between them resize them
	// (dragged, or from the keyboard; a double-click makes them even again). Dragging a header
	// sideways moves its pane among the others. A press or the keyboard focus in a pane puts
	// the focus on it: the view bar, the side panels and the commands act on that one, whose
	// header carries the accent (no ring around the pane). While the lock is on, panes of the
	// same document take the rotation of the one being scrolled and the zoom of the one
	// zoomed; with synchronized zoom, every pane takes that zoom (sync.ts).
	import { X } from '@lucide/svelte';
	import { untrack } from 'svelte';

	import FileBanner from '#lib/components/FileBanner.svelte';
	import PdfaBanner from '#lib/components/PdfaBanner.svelte';
	import AnnotationPropertiesDialog from '#lib/features/annotations/AnnotationPropertiesDialog.svelte';
	import AnnotationToolbar from '#lib/features/annotations/AnnotationToolbar.svelte';
	import BookmarkInspector from '#lib/features/bookmarks/BookmarkInspector.svelte';
	import OrganizeView from '#lib/features/organize/OrganizeView.svelte';
	import Viewer from '#lib/features/viewer/Viewer.svelte';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';
	import type { TabGroup } from '#lib/stores/group.svelte.ts';

	import DropPreview from './DropPreview.svelte';
	import { equalWidths, resizeWidths } from './split.ts';
	import { sameDocument, zoomed } from './sync.ts';
	import { dropTarget } from './tabdrop.svelte.ts';

	interface Props {
		group: TabGroup;
		/** Full screen: the pages alone, without the panes' headers and banners. */
		full: boolean;
		/** The annotation toolbar floats over the pages. */
		floatingTools: boolean;
	}

	let { group, full, floatingTools }: Props = $props();

	const focused = $derived(group.focused);

	let row: HTMLDivElement | undefined = $state();

	// Panes of one document follow the lead's rotation while the lock is on.
	$effect(() => {
		if (!group.locked) return;
		const lead = group.lead ?? group.focused;
		if (!lead) return;
		void [lead.rotation, lead.pageTurns];
		untrack(() => sameDocument(group));
	});

	// The pane zoomed leads the zoom: of every pane with synchronized zoom, of the panes of
	// its document while the lock is on.
	$effect(() => {
		if (!group.locked && !group.zoomLocked) return;
		for (const view of group.views) void [view.zoom, view.zoomMode];
		untrack(() => zoomed(group));
	});

	// ----- dividers -----

	let resizing: { index: number; pointer: number; x: number; widths: number[] } | null = $state(null);

	function onDividerDown(event: PointerEvent, index: number) {
		if (event.button !== 0) return;
		event.preventDefault();
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
		resizing = { index, pointer: event.pointerId, x: event.clientX, widths: [...group.widths] };
	}

	function onDividerMove(event: PointerEvent) {
		if (!resizing || event.pointerId !== resizing.pointer || !row) return;
		group.widths = resizeWidths(resizing.widths, resizing.index, event.clientX - resizing.x, row.clientWidth);
	}

	function onDividerUp(event: PointerEvent) {
		if (resizing?.pointer === event.pointerId) resizing = null;
	}

	/** The arrows move a divider 16 px (64 with Ctrl). */
	function onDividerKey(event: KeyboardEvent, index: number) {
		if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
		if (!row) return;
		event.preventDefault();
		const step = (event.ctrlKey ? 64 : 16) * (event.key === 'ArrowLeft' ? -1 : 1);
		group.widths = resizeWidths(group.widths, index, step, row.clientWidth);
	}

	/** The share, in percent, of the panes before the divider: what screen readers hear. */
	function dividerValue(index: number): number {
		return Math.round(group.widths.slice(0, index + 1).reduce((a, b) => a + b, 0) * 100);
	}

	// ----- moving a pane by its header -----

	const DRAG_THRESHOLD = 4;
	let moving: { view: DocTab; pointer: number; x: number; moved: boolean; to: number } | null = $state(null);

	function onHeaderDown(event: PointerEvent, view: DocTab) {
		if (event.button !== 0 || (event.target as HTMLElement).closest('button')) return;
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
		moving = { view, pointer: event.pointerId, x: event.clientX, moved: false, to: group.views.indexOf(view) };
	}

	function onHeaderMove(event: PointerEvent) {
		if (!moving || event.pointerId !== moving.pointer || !row) return;
		if (!moving.moved && Math.abs(event.clientX - moving.x) < DRAG_THRESHOLD) return;
		moving.moved = true;
		// The gap nearest the pointer: before the pane whose middle is past it.
		const panes = [...row.querySelectorAll<HTMLElement>('[data-split-pane]')];
		let to = panes.length;
		for (let i = 0; i < panes.length; i++) {
			const r = panes[i]!.getBoundingClientRect();
			if (event.clientX < r.left + r.width / 2) {
				to = i;
				break;
			}
		}
		moving.to = to;
	}

	function onHeaderUp(event: PointerEvent) {
		const done = moving;
		if (!done || event.pointerId !== done.pointer) return;
		moving = null;
		if (!done.moved) return;
		const from = group.views.indexOf(done.view);
		let to = done.to;
		if (to > from) to--;
		if (to === from) return;
		const views = [...group.views];
		views.splice(from, 1);
		views.splice(to, 0, done.view);
		group.setViews(views, done.view);
	}

	function onHeaderKey(event: KeyboardEvent) {
		if (event.key === 'Escape' && moving) {
			event.preventDefault();
			event.stopPropagation();
			moving = null;
		}
	}

	/** Where a pane being moved would go, as a line between the panes. */
	const markAt = $derived.by(() => {
		if (!moving?.moved) return null;
		const from = group.views.indexOf(moving.view);
		return moving.to === from || moving.to === from + 1 ? null : moving.to;
	});

	/** The left edge of gap `at` among the panes, as a share of the row. */
	function gapLeft(at: number): number {
		return group.widths.slice(0, at).reduce((a, b) => a + b, 0) * 100;
	}
</script>

<div class="split-row relative flex min-h-0 min-w-0 flex-1" bind:this={row} {@attach dropTarget}>
	{#each group.views as view, i (view.key)}
		{@const name = group.names[i] ?? view.name}
		{@const isFocused = view === focused}
		{#if i > 0}
			<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
			<div
				class="split-divider"
				class:split-divider-active={resizing?.index === i - 1}
				role="separator"
				aria-orientation="vertical"
				aria-label="Resize {group.names[i - 1]} and {name}"
				aria-valuemin={0}
				aria-valuemax={100}
				aria-valuenow={dividerValue(i - 1)}
				tabindex="0"
				data-tip="Drag to resize; double-click to make them even"
				onpointerdown={(e) => onDividerDown(e, i - 1)}
				onpointermove={onDividerMove}
				onpointerup={onDividerUp}
				onpointercancel={onDividerUp}
				onkeydown={(e) => onDividerKey(e, i - 1)}
				ondblclick={() => (group.widths = equalWidths(group.views.length))}
			></div>
		{/if}
		<section
			class="split-pane flex min-h-0 min-w-0 flex-col"
			class:split-pane-focused={isFocused}
			style:flex="{group.widths[i] ?? 1} 1 0px"
			aria-label={name}
			data-split-pane={view.key}
			onpointerdowncapture={() => app.focusView(view)}
			onfocusin={() => app.focusView(view)}
		>
			{#if !full}
				<!-- svelte-ignore a11y_no_static_element_interactions -->
				<div
					class="split-header"
					class:split-header-focused={isFocused}
					class:split-header-moving={moving?.moved && moving.view === view}
					data-tip={view.untitled ? 'Not saved yet' : view.path}
					onpointerdown={(e) => onHeaderDown(e, view)}
					onpointermove={onHeaderMove}
					onpointerup={onHeaderUp}
					onpointercancel={() => (moving = null)}
					onkeydown={onHeaderKey}
				>
					<span class="min-w-0 flex-1 truncate">{name}</span>
					{#if view.saving}
						<span class="size-3 shrink-0 animate-spin rounded-full border-2 border-fg-muted border-t-transparent" aria-label="saving" role="img"></span>
					{:else if view.state.dirty}
						<span class="size-2 shrink-0 rounded-full bg-fg" aria-label="unsaved changes" data-tip="Unsaved changes" role="img"></span>
					{/if}
					<button
						type="button"
						class="icon-button split-header-close shrink-0"
						aria-label="Show {name} in a tab of its own"
						data-tip={group.views.some((v) => v !== view && v.id === view.id) ? 'Close this view' : 'Show in a tab of its own'}
						onclick={() => app.takeOut(view)}
					>
						<X size={14} aria-hidden="true" />
					</button>
				</div>
				<div class="shrink-0">
					<FileBanner tab={view} />
					<PdfaBanner tab={view} />
				</div>
			{/if}
			{#if view.organize}
				<OrganizeView tab={view} organize={view.organize} />
			{:else}
				<div class="relative flex min-h-0 min-w-0 flex-1 flex-col">
					<Viewer tab={view} />
				</div>
			{/if}
		</section>
	{/each}
	{#if markAt !== null}
		<div class="split-move-mark" style:left="{gapLeft(markAt)}%" aria-hidden="true"></div>
	{/if}
	{#if focused && !focused.organize}
		{#if floatingTools}
			<AnnotationToolbar tab={focused} />
		{/if}
		{#key focused.key}
			<AnnotationPropertiesDialog tab={focused} />
			<BookmarkInspector tab={focused} />
		{/key}
	{/if}
	<DropPreview />
</div>
