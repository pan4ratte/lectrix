<script lang="ts">
	// Left sidebar (section 8): Pages (thumbnails), Bookmarks, Annotations and Page labels.
	// Four tabs don't fit the sidebar's width as words, so they are icons with tooltips and
	// accessible names.
	import { Bookmark, GalleryVertical, MessageSquareText, Tag } from '@lucide/svelte';
	import { Tabs } from 'bits-ui';

	import AnnotationsPanel from '#lib/features/annotations/AnnotationsPanel.svelte';
	import BookmarksPanel from '#lib/features/bookmarks/BookmarksPanel.svelte';
	import LabelsPanel from '#lib/features/labels/LabelsPanel.svelte';
	import Thumbnails from '#lib/features/viewer/Thumbnails.svelte';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	let { tab }: { tab: DocTab } = $props();

	const panels = [
		{ value: 'pages', label: 'Pages', icon: GalleryVertical },
		{ value: 'bookmarks', label: 'Bookmarks', icon: Bookmark },
		{ value: 'annotations', label: 'Annotations', icon: MessageSquareText },
		{ value: 'labels', label: 'Page labels', icon: Tag }
	] as const;

	const needing = $derived(tab.allAnnotations.some((a) => a.problems.length > 0));
</script>

<aside class="flex w-60 shrink-0 flex-col border-r border-line bg-chrome" aria-label="Sidebar">
	<Tabs.Root bind:value={app.sidebarPanel} class="flex min-h-0 flex-1 flex-col">
		<Tabs.List class="flex shrink-0 gap-1 px-2 pt-2" aria-label="Sidebar panels">
			{#each panels as p (p.value)}
				{@const Icon = p.icon}
				<Tabs.Trigger value={p.value} class="sidebar-tab relative" aria-label={p.label} title={p.label}>
					<Icon size={18} aria-hidden="true" />
					{#if p.value === 'annotations' && needing}
						<span class="absolute top-1 right-1 h-2 w-2 rounded-full bg-danger" aria-hidden="true"></span>
					{/if}
				</Tabs.Trigger>
			{/each}
		</Tabs.List>
		<Tabs.Content value="pages" class="min-h-0 flex-1">
			{#key tab.id}
				<Thumbnails {tab} />
			{/key}
		</Tabs.Content>
		<Tabs.Content value="bookmarks" class="min-h-0 flex-1">
			{#key tab.id}
				<BookmarksPanel {tab} />
			{/key}
		</Tabs.Content>
		<Tabs.Content value="annotations" class="min-h-0 flex-1">
			{#key tab.id}
				<AnnotationsPanel {tab} />
			{/key}
		</Tabs.Content>
		<Tabs.Content value="labels" class="min-h-0 flex-1">
			{#key tab.id}
				<LabelsPanel {tab} />
			{/key}
		</Tabs.Content>
	</Tabs.Root>
</aside>
