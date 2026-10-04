<script lang="ts">
	// Asks for the password of an encrypted document (section 5.4).
	import { Lock } from '@lucide/svelte';
	import { Dialog } from 'bits-ui';

	import { app } from '#lib/stores/app.svelte.ts';

	const prompt = $derived(app.passwordPrompts[0] ?? null);
	let password = $state('');

	function submit(event: SubmitEvent) {
		event.preventDefault();
		if (!prompt) return;
		const value = password;
		password = '';
		void app.submitPassword(prompt.token, value);
	}

	function cancel() {
		if (!prompt) return;
		password = '';
		void app.submitPassword(prompt.token, null);
	}
</script>

<Dialog.Root
	open={prompt !== null}
	onOpenChange={(open) => {
		if (!open) cancel();
	}}
>
	<Dialog.Portal>
		<Dialog.Overlay class="dialog-overlay" />
		<Dialog.Content class="dialog-content w-[400px]">
			{#if prompt}
				<div class="flex items-center gap-2">
					<Lock size={18} aria-hidden="true" />
					<Dialog.Title class="text-base font-semibold">Password required</Dialog.Title>
				</div>
				<Dialog.Description class="mt-2 text-sm text-fg-muted">
					{prompt.retry
						? `That password didn’t open ${prompt.name}. Check it and try again.`
						: `${prompt.name} is protected. Enter its password to open it.`}
				</Dialog.Description>
				<form class="mt-4 flex flex-col gap-4" onsubmit={submit}>
					<input
						type="password"
						class="field h-8"
						class:border-line={!prompt.retry}
						class:border-danger={prompt.retry}
						bind:value={password}
						aria-label="Password"
						aria-invalid={prompt.retry}
						autocomplete="off"
					/>
					<div class="flex justify-end gap-2">
						<button type="button" class="button" onclick={cancel}>Cancel</button>
						<button type="submit" class="button button-primary">Open</button>
					</div>
				</form>
			{/if}
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
