<script lang="ts">
	// "Rotate pages…": turns pages in the document itself (sets /Rotate; undoable). Rotating
	// the view (View menu) is separate and does not change the file.
	import { Dialog } from 'bits-ui';

	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { parsePageRanges } from './ranges.ts';

	let { tab }: { tab: DocTab } = $props();

	let which = $state<'current' | 'all' | 'range'>('current');
	let rangeText = $state('');
	let degrees = $state(90);
	let error = $state('');

	function pages(): number[] | null {
		if (which === 'current') return [tab.currentPage];
		if (which === 'all') return tab.pages.map((_, i) => i);
		return parsePageRanges(rangeText, tab.pageCount);
	}

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		const list = pages();
		if (!list || list.length === 0) {
			error = `Enter pages between 1 and ${tab.pageCount}, for example 1-3, 7.`;
			return;
		}
		app.rotateDialogOpen = false;
		await app.rotatePages(tab, list, degrees);
	}
</script>

<Dialog.Root bind:open={app.rotateDialogOpen}>
	<Dialog.Portal>
		<Dialog.Overlay class="dialog-overlay" />
		<Dialog.Content class="dialog-content w-96">
			<Dialog.Title class="text-base font-semibold">Rotate pages</Dialog.Title>
			<Dialog.Description class="mt-1 text-sm text-fg-muted">
				This changes the document. To turn only your view, use View &gt; Rotate view.
			</Dialog.Description>
			<form class="mt-4 flex flex-col gap-4" onsubmit={submit}>
				<fieldset class="flex flex-col gap-2">
					<legend class="mb-1 text-sm font-medium">Pages</legend>
					<label class="flex items-center gap-2">
						<input type="radio" bind:group={which} value="current" />
						Current page ({tab.currentPage + 1})
					</label>
					<label class="flex items-center gap-2">
						<input type="radio" bind:group={which} value="all" />
						All pages
					</label>
					<label class="flex items-center gap-2">
						<input type="radio" bind:group={which} value="range" />
						Pages
						<input
							class="h-7 flex-1 rounded-control border border-line bg-surface px-2 outline-none focus:border-accent"
							bind:value={rangeText}
							onfocus={() => (which = 'range')}
							placeholder="1-3, 7"
							aria-label="Page numbers"
						/>
					</label>
				</fieldset>
				<fieldset class="flex flex-col gap-2">
					<legend class="mb-1 text-sm font-medium">Direction</legend>
					<label class="flex items-center gap-2">
						<input type="radio" bind:group={degrees} value={90} />
						Clockwise 90°
					</label>
					<label class="flex items-center gap-2">
						<input type="radio" bind:group={degrees} value={-90} />
						Counter-clockwise 90°
					</label>
					<label class="flex items-center gap-2">
						<input type="radio" bind:group={degrees} value={180} />
						180°
					</label>
				</fieldset>
				{#if error}
					<p class="text-sm text-danger" role="alert">{error}</p>
				{/if}
				<div class="flex justify-end gap-2">
					<Dialog.Close class="button">Cancel</Dialog.Close>
					<button type="submit" class="button button-primary">Rotate</button>
				</div>
			</form>
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
