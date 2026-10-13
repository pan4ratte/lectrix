<script lang="ts">
	// The bar at the top of the document, as in Acrobat (section 8), centered: the page and
	// zoom controls (PageControls.svelte), the button that shows the pages as a grid to
	// organize (section 6.7), then the annotation tools when Settings dock them
	// here (docked at a side of the pages, they have a bar of their own, App.svelte). Full
	// screen hides it, and its controls go to the floating toolbar (section 6.1).
	import { LayoutGrid } from '@lucide/svelte';

	import AnnotationToolbar from '#lib/features/annotations/AnnotationToolbar.svelte';
	import { organizePages } from '#lib/features/organize/actions.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import PageControls from './PageControls.svelte';

	let { tab }: { tab: DocTab } = $props();

	const docked = $derived(app.settings?.toolbarStyle === 'panel' && app.settings.toolbarDock === 'top');
</script>

<div
	class="flex min-h-[40px] shrink-0 flex-wrap items-center justify-center gap-x-2 gap-y-1 border-b border-line bg-chrome px-2 py-1 text-sm text-fg-muted"
	data-view-bar
>
	<PageControls {tab} />
	<button
		type="button"
		class="icon-button"
		aria-label="Organize pages"
		data-tip={app.hint('Organize pages', 'organizePages')}
		onclick={() => void organizePages(tab)}
	>
		<LayoutGrid size={16} aria-hidden="true" />
	</button>

	{#if docked}
		<span class="h-6 w-px bg-line" aria-hidden="true"></span>
		<AnnotationToolbar {tab} dock="top" />
	{/if}
</div>
