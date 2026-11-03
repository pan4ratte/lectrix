<script lang="ts">
	// The bar over the area the Snapshot tool framed (section 6.14): copy it as an image, or
	// save it as one through the Export as images dialog.
	import { Copy, ImageDown } from '@lucide/svelte';

	import type { Area } from '#lib/features/annotations/bars.ts';
	import FloatingBar from '#lib/features/annotations/FloatingBar.svelte';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { copyArea, saveArea } from './actions.ts';

	interface Props {
		tab: DocTab;
		anchor: Area;
		view: Area;
		contentWidth: number;
	}

	let { tab, anchor, view, contentWidth }: Props = $props();
</script>

<FloatingBar label="Snapshot actions" {anchor} {view} {contentWidth} prefer="above">
	<button
		type="button"
		class="icon-button"
		aria-label="Copy as image"
		data-tip={app.hint('Copy as image', 'copy')}
		data-snapshot-copy
		onclick={() => void copyArea(tab).then(() => tab.viewer?.focus())}
	>
		<Copy size={18} aria-hidden="true" />
	</button>
	<button
		type="button"
		class="icon-button"
		aria-label="Save as image…"
		data-tip="Save as image…"
		data-snapshot-save
		onclick={() => saveArea(tab)}
	>
		<ImageDown size={18} aria-hidden="true" />
	</button>
</FloatingBar>
