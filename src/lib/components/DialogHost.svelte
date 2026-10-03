<script lang="ts">
	// Shows the app's modal questions (unsaved changes, signed documents...).
	import { AlertDialog } from 'bits-ui';

	import { app } from '#lib/stores/app.svelte.ts';

	const request = $derived(app.dialog);
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
			{#if request}
				<AlertDialog.Title class="text-base font-semibold">{request.title}</AlertDialog.Title>
				<AlertDialog.Description class="mt-2">
					<p>{request.message}</p>
					{#if request.detail}
						<p class="mt-2 text-sm text-fg-muted">{request.detail}</p>
					{/if}
				</AlertDialog.Description>
				<div class="mt-6 flex justify-end gap-2">
					{#each request.buttons as button (button.id)}
						<button
							type="button"
							class="button"
							class:button-primary={button.primary}
							onclick={() => request.resolve(button.id)}
						>
							{button.label}
						</button>
					{/each}
				</div>
			{/if}
		</AlertDialog.Content>
	</AlertDialog.Portal>
</AlertDialog.Root>
