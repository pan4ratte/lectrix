<script lang="ts">
	// Inspector for the selected bookmark (section 8: shown only while something is
	// selected). Opened with "Properties" from the bookmark's context menu; it then follows
	// the selection until closed. It floats over the page canvas, so opening it never
	// changes the zoom of a fit-width view.
	import { ArrowRight, Copy, Crosshair, Trash, X } from '@lucide/svelte';

	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { copyLink, deleteBookmark, goToBookmark, renameBookmark, setDestinationHere } from './actions.ts';
	import { describeTarget, descendantCount, locate } from './tree.ts';

	let { tab }: { tab: DocTab } = $props();

	const bookmark = $derived(
		tab.selectedBookmark !== null ? (locate(tab.outline.items, tab.selectedBookmark)?.bookmark ?? null) : null
	);
	const editable = $derived(tab.canEditBookmarks);
	const inside = $derived(bookmark ? descendantCount(bookmark) : 0);

	function commit(event: Event) {
		if (!bookmark) return;
		const input = event.currentTarget as HTMLInputElement;
		void renameBookmark(tab, bookmark.id, input.value);
	}

	function onTitleKey(event: KeyboardEvent) {
		event.stopPropagation();
		if (event.key === 'Enter') {
			event.preventDefault();
			commit(event);
		} else if (event.key === 'Escape') {
			event.preventDefault();
			(event.currentTarget as HTMLInputElement).value = bookmark?.title ?? '';
		}
	}
</script>

{#if app.inspectorOpen && bookmark}
	{@const b = bookmark}
	<aside
		class="absolute right-6 z-10 flex w-72 flex-col gap-3 rounded-panel border border-line bg-surface-raised p-3 shadow-[0_4px_12px_var(--color-page-shadow)]"
		style:top={tab.search.open ? '64px' : '12px'}
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
					class="h-8 rounded-control border border-line bg-surface px-2 text-sm text-fg outline-none focus:border-accent"
					value={b.title}
					readonly={!editable}
					onkeydown={onTitleKey}
					onblur={commit}
				/>
			{/key}
		</label>
		<div class="flex flex-col gap-1">
			<span class="text-xs text-fg-muted">Destination</span>
			<p class="text-sm break-words select-text">{describeTarget(b.target, tab.labels)}</p>
			{#if inside > 0}
				<p class="text-xs text-fg-muted">Contains {inside === 1 ? '1 bookmark' : `${inside} bookmarks`}.</p>
			{/if}
		</div>
		<div class="flex flex-wrap gap-2">
			{#if b.target.kind === 'page'}
				<button type="button" class="button gap-1" onclick={() => goToBookmark(tab, b.id)}>
					<ArrowRight size={14} aria-hidden="true" />Go to
				</button>
			{/if}
			{#if b.target.kind === 'uri'}
				{@const uri = b.target.uri}
				<button type="button" class="button gap-1" onclick={() => void copyLink(uri)}>
					<Copy size={14} aria-hidden="true" />Copy link
				</button>
			{/if}
			<button
				type="button"
				class="button gap-1"
				disabled={!editable}
				onclick={() => void setDestinationHere(tab, b.id)}
				title="Point this bookmark at what is shown now"
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
{/if}
