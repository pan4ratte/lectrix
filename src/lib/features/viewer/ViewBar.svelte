<script lang="ts">
	// The bar at the top of the document, as in Acrobat (section 8), centered: the page and
	// zoom controls (PageControls.svelte), then the annotation tools when Settings dock them
	// here. Full screen hides it, and its controls go to the floating toolbar (section 6.1).
	import AnnotationToolbar from '#lib/features/annotations/AnnotationToolbar.svelte';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import PageControls from './PageControls.svelte';

	let { tab }: { tab: DocTab } = $props();

	const docked = $derived(app.settings?.toolbarStyle === 'panel');
</script>

<div
	class="flex min-h-[40px] shrink-0 flex-wrap items-center justify-center gap-x-2 gap-y-1 border-b border-line bg-chrome px-2 py-1 text-sm text-fg-muted"
	data-view-bar
>
	<PageControls {tab} />

	{#if docked}
		<span class="h-6 w-px bg-line" aria-hidden="true"></span>
		<AnnotationToolbar {tab} docked />
	{/if}
</div>
