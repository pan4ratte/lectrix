<script lang="ts">
	// Shows the app's modal questions (unsaved changes, signed documents...). On macOS as an
	// alert of the system's (section 8): the app's icon over the question, centred, and the
	// buttons as it places them (alert.ts), Return pressing the main one.
	import { AlertDialog } from 'bits-ui';

	import iconUrl from '#lib/assets/lectrix-icon.svg';
	import { PLATFORM } from '#lib/platform.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	import { macAlertLayout } from './alert.ts';
	import { held } from './held.svelte.ts';

	const request = $derived(app.dialog);
	/** The question on screen: the one last asked, which stays while the dialog goes. */
	const asked = held(() => app.dialog);
	const shown = $derived(asked.current);

	const mac = PLATFORM === 'macos';
	const layout = $derived(shown && mac ? macAlertLayout(shown.buttons, shown.cancel) : null);
	const buttons = $derived(layout?.buttons ?? shown?.buttons ?? []);
	let mainButton = $state<HTMLButtonElement | null>(null);
</script>

<AlertDialog.Root
	open={request !== null}
	onOpenChange={(open) => {
		if (!open && request) request.resolve(request.cancel);
	}}
>
	<AlertDialog.Portal>
		<AlertDialog.Overlay class="dialog-overlay" />
		<AlertDialog.Content
			class={['dialog-content', mac ? 'mac-alert' : 'w-[440px]']}
			onOpenAutoFocus={(event) => {
				if (!mac || !mainButton) return;
				event.preventDefault();
				mainButton.focus();
			}}
		>
			{#if shown}
				{#if mac}
					<img src={iconUrl} alt="" width="64" height="64" class="mx-auto" draggable="false" />
				{/if}
				<AlertDialog.Title class={mac ? 'mac-alert-title' : 'text-base font-semibold'}>{shown.title}</AlertDialog.Title>
				<AlertDialog.Description class={mac ? 'mac-alert-message' : 'mt-2'}>
					<p>{shown.message}</p>
					{#if shown.detail}
						<p class={mac ? 'mt-1.5 text-fg-muted' : 'mt-2 text-sm text-fg-muted'}>{shown.detail}</p>
					{/if}
				</AlertDialog.Description>
				<div class={mac ? ['mac-alert-buttons', layout?.stacked && 'mac-alert-stacked'] : 'mt-6 flex justify-end gap-2'}>
					{#each buttons as button (button.id)}
						<!-- (Once answered, the question is gone, and a second press does nothing.) -->
						{#if button === layout?.main}
							<button
								type="button"
								class="button"
								class:button-primary={button.primary}
								class:button-danger={button.danger}
								bind:this={mainButton}
								onclick={() => request?.resolve(button.id)}
							>
								{button.label}
							</button>
						{:else}
							<button
								type="button"
								class="button"
								class:button-primary={button.primary}
								class:button-danger={button.danger}
								onclick={() => request?.resolve(button.id)}
							>
								{button.label}
							</button>
						{/if}
					{/each}
				</div>
			{/if}
		</AlertDialog.Content>
	</AlertDialog.Portal>
</AlertDialog.Root>
