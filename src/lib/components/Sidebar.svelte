<script lang="ts">
	// Left sidebar (section 8): Pages (thumbnails), Bookmarks and Page labels. Annotations
	// joins them in Phase 5. The tabs are text only: with icons, three no longer fit the
	// sidebar's width.
	import { Tabs } from 'bits-ui';

	import BookmarksPanel from '#lib/features/bookmarks/BookmarksPanel.svelte';
	import LabelsPanel from '#lib/features/labels/LabelsPanel.svelte';
	import Thumbnails from '#lib/features/viewer/Thumbnails.svelte';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	let { tab }: { tab: DocTab } = $props();
</script>

<aside class="flex w-60 shrink-0 flex-col border-r border-line bg-chrome" aria-label="Sidebar">
	<Tabs.Root bind:value={app.sidebarPanel} class="flex min-h-0 flex-1 flex-col">
		<Tabs.List class="flex shrink-0 gap-1 px-2 pt-2" aria-label="Sidebar panels">
			<Tabs.Trigger value="pages" class="sidebar-tab">Pages</Tabs.Trigger>
			<Tabs.Trigger value="bookmarks" class="sidebar-tab">Bookmarks</Tabs.Trigger>
			<Tabs.Trigger value="labels" class="sidebar-tab" title="Page labels">Labels</Tabs.Trigger>
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
</aside>
