<script lang="ts">
	// Tells the user that another program changed or removed the open file (section 7).
	import { TriangleAlert } from '@lucide/svelte';

	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	let { tab }: { tab: DocTab } = $props();

	async function reloadDiscarding() {
		const choice = await app.ask({
			title: 'Discard your changes?',
			message: `Reloading ${tab.name} replaces your unsaved changes with the version on disk.`,
			buttons: [
				{ id: 'reload', label: 'Reload' },
				{ id: 'cancel', label: 'Cancel', primary: true }
			],
			cancel: 'cancel'
		});
		if (choice === 'reload') await app.reload(tab);
	}
</script>

{#if tab.banner}
	<div class="flex items-center gap-3 border-b border-line bg-info-bg px-4 py-2" role="alert">
		<TriangleAlert size={16} aria-hidden="true" class="shrink-0" />
		<p class="min-w-0 flex-1">
			{#if tab.banner === 'changedOnDisk'}
				Another program changed this file.
			{:else if tab.banner === 'changedOnDiskDirty'}
				Another program changed this file, and you have unsaved changes here. Save your
				version under a new name to keep both.
			{:else}
				This file was moved or deleted. Save it to keep your copy.
			{/if}
		</p>
		{#if tab.banner === 'changedOnDisk'}
			<button type="button" class="button button-primary" onclick={() => void app.reload(tab)}>Reload</button>
		{:else if tab.banner === 'changedOnDiskDirty'}
			<button type="button" class="button button-primary" onclick={() => void app.saveAs(tab)}>Save as…</button>
			<button type="button" class="button" onclick={() => void reloadDiscarding()}>Reload</button>
		{:else}
			<button type="button" class="button button-primary" onclick={() => void app.saveAs(tab)}>Save as…</button>
		{/if}
		<button type="button" class="button" onclick={() => (tab.banner = null)}>Dismiss</button>
	</div>
{/if}
