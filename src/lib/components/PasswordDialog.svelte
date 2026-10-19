<script lang="ts">
	// Asks for the password of an encrypted document (section 5.4), to open it, to remove
	// the password (Unlock, ADR 0025) or to change its protection (Protect, ADR 0026).
	import { Lock } from '@lucide/svelte';
	import { Dialog } from 'bits-ui';

	import { app } from '#lib/stores/app.svelte.ts';

	import { held } from './held.svelte.ts';

	/** The password asked for now. */
	const asked = $derived(app.passwordPrompts[0] ?? null);
	/** What the dialog shows: the one last asked for, which stays while the dialog goes. */
	const last = held(() => asked);
	const prompt = $derived(last.current);
	let password = $state('');

	function submit(event: SubmitEvent) {
		event.preventDefault();
		if (!asked) return;
		const value = password;
		password = '';
		void app.submitPassword(asked.token, value);
	}

	function cancel() {
		if (!asked) return;
		password = '';
		void app.submitPassword(asked.token, null);
	}
</script>

<Dialog.Root
	open={asked !== null}
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
						: prompt.purpose === 'unlock'
							? `${prompt.name} is protected. Enter its password to remove it.`
							: prompt.purpose === 'protect'
								? `${prompt.name} is protected. Enter its password to change its protection.`
								: prompt.purpose === 'compress'
									? `${prompt.name} is protected. Enter its password to compress it.`
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
						<button type="submit" class="button button-primary">
							{prompt.purpose === 'unlock'
								? 'Unlock'
								: prompt.purpose === 'protect' || prompt.purpose === 'compress'
									? 'Continue'
									: 'Open'}
						</button>
					</div>
				</form>
			{/if}
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
