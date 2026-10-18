<script lang="ts">
	// Where a tab dragged over the pages would go beside the tab shown (section 6.1, split
	// view): the column it would have once joined, tinted in the accent. Lies over the pages,
	// which must be its positioned parent; takes no pointer events.
	import { tabDrop } from './tabdrop.svelte.ts';

	const columns = $derived(tabDrop.panes + 1);
</script>

{#if tabDrop.at !== null}
	<div
		class="split-drop"
		style:left="{(tabDrop.at / columns) * 100}%"
		style:width="{100 / columns}%"
		aria-hidden="true"
		data-split-drop={tabDrop.at}
	></div>
{/if}
