<script lang="ts">
	// Shows the app's modal questions (unsaved changes, signed documents...).
	import { AlertDialog } from 'bits-ui';

	import { app } from '#lib/stores/app.svelte.ts';

	import { held } from './held.svelte.ts';

	const request = $derived(app.dialog);
	/** The question on screen: the one last asked, which stays while the dialog goes. */
	const asked = held(() => app.dialog);
	const shown = $derived(asked.current);
</script>

<AlertDialog.Root
	open={request !== null}
	onOpenChange={(open) => {
		if (!open && request) request.resolve(request.cancel);
	}}
>
	<AlertDialog.Portal>
		<AlertDialog.Overlay class="dialog-overlay" />
		<AlertDialog.Content class="dialog-content w-[440px]">
			{#if shown}
				<AlertDialog.Title class="text-base font-semibold">{shown.title}</AlertDialog.Title>
				<AlertDialog.Description class="mt-2">
					<p>{shown.message}</p>
					{#if shown.detail}
						<p class="mt-2 text-sm text-fg-muted">{shown.detail}</p>
					{/if}
				</AlertDialog.Description>
				<div class="mt-6 flex justify-end gap-2">
					{#each shown.buttons as button (button.id)}
						<!-- (Once answered, the question is gone, and a second press does nothing.) -->
						<button
							type="button"
							class="button"
							class:button-primary={button.primary}
							class:button-danger={button.danger}
							onclick={() => request?.resolve(button.id)}
						>
							{button.label}
						</button>
					{/each}
				</div>
			{/if}
		</AlertDialog.Content>
	</AlertDialog.Portal>
</AlertDialog.Root>
