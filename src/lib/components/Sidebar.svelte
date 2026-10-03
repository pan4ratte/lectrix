<script lang="ts">
	// Left sidebar. Phase 1 has the Thumbnails panel; Bookmarks, Annotations and Page labels
	// join it in later phases (section 8).
	import Thumbnails from '#lib/features/viewer/Thumbnails.svelte';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	let { tab }: { tab: DocTab } = $props();
</script>

<aside class="flex w-48 shrink-0 flex-col border-r border-line bg-chrome" aria-label="Sidebar">
	<h2 class="px-3 pt-2 pb-1 text-xs font-semibold tracking-wide text-fg-muted uppercase">Pages</h2>
	<div class="min-h-0 flex-1">
		{#key tab.id}
			<Thumbnails {tab} />
		{/key}
	</div>
</aside>
