<script lang="ts">
	// Tells the user that the file claims PDF/A and is therefore open for reading only, as
	// Acrobat opens such files, and offers to enable editing (section 5.4).
	import { Info } from '@lucide/svelte';

	import { pdfaName } from '#lib/format.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	let { tab }: { tab: DocTab } = $props();

	const level = $derived(tab.fileFlags.pdfa ? ` (${pdfaName(tab.fileFlags.pdfa)})` : '');

	async function enable() {
		await app.enableEditing(tab);
		// The button goes with the banner: the keyboard carries on in the pages.
		if (!tab.readOnly) tab.viewer?.focus();
	}
</script>

{#if tab.readOnly}
	<div class="flex items-center gap-3 border-b border-line bg-info-bg px-4 py-2" role="status" data-pdfa-banner>
		<Info size={16} aria-hidden="true" class="shrink-0" />
		<p class="min-w-0 flex-1">
			This file claims compliance with the PDF/A standard{level}, so it was opened for reading only.
			Saving annotations or inserted pages removes that claim.
		</p>
		<button type="button" class="button button-primary" onclick={() => void enable()}>Enable editing</button>
	</div>
{/if}
