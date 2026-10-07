<script lang="ts">
	// The scroll modes as icons, drawn like Lucide's (24-unit grid, 2-unit round strokes):
	// one page or two, with the edges of the pages before and after when they scroll on.
	import type { ViewMode } from './layout.ts';

	let { mode, size = 16 }: { mode: ViewMode; size?: number } = $props();
</script>

<svg
	width={size}
	height={size}
	viewBox="0 0 24 24"
	fill="none"
	stroke="currentColor"
	stroke-width="2"
	stroke-linecap="round"
	stroke-linejoin="round"
	aria-hidden="true"
>
	{#if mode === 'singlePage'}
		<rect x="6" y="3" width="12" height="18" rx="2" />
	{:else if mode === 'singlePageContinuous'}
		<rect x="6" y="6" width="12" height="12" rx="2" />
		<path d="M8 2h8M8 22h8" />
	{:else if mode === 'twoPage'}
		<rect x="2" y="4" width="9" height="16" rx="2" />
		<rect x="13" y="4" width="9" height="16" rx="2" />
	{:else}
		<rect x="2" y="6" width="9" height="12" rx="2" />
		<rect x="13" y="6" width="9" height="12" rx="2" />
		<path d="M4 2h5M15 2h5M4 22h5M15 22h5" />
	{/if}
</svg>
