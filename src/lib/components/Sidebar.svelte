<script lang="ts">
	// Left sidebar (section 8): Pages (thumbnails), Bookmarks and Page labels; the annotation
	// list has the right pane. The tabs are icons with tooltips and accessible names, so
	// they fit the narrowest sidebar.
	import { Bookmark, GalleryVertical, Tag } from '@lucide/svelte';
	import { Tabs } from 'bits-ui';

	import BookmarksPanel from '#lib/features/bookmarks/BookmarksPanel.svelte';
	import LabelsPanel from '#lib/features/labels/LabelsPanel.svelte';
	import Thumbnails from '#lib/features/viewer/Thumbnails.svelte';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	let { tab }: { tab: DocTab } = $props();

	const panels = [
		{ value: 'pages', label: 'Pages', icon: GalleryVertical },
		{ value: 'bookmarks', label: 'Bookmarks', icon: Bookmark },
		{ value: 'labels', label: 'Page labels', icon: Tag }
	] as const;
</script>

<Tabs.Root bind:value={app.sidebarPanel} class="flex min-h-0 flex-1 flex-col">
	<Tabs.List class="flex shrink-0 gap-1 px-2 pt-2" aria-label="Sidebar panels">
		{#each panels as p (p.value)}
			{@const Icon = p.icon}
			<Tabs.Trigger value={p.value} class="sidebar-tab relative" aria-label={p.label} title={p.label}>
				<Icon size={18} aria-hidden="true" />
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
	<Tabs.Content value="labels" class="min-h-0 flex-1">
		{#key tab.id}
			<LabelsPanel {tab} />
		{/key}
	</Tabs.Content>
</Tabs.Root>
