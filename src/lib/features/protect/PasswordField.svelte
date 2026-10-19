<script lang="ts">
	// A password field of the Protect dialog (section 6.9), with a button at its end that
	// shows or hides what was typed.
	import { Eye, EyeOff } from '@lucide/svelte';

	interface Props {
		value: string;
		id: string;
		disabled?: boolean;
		invalid?: boolean;
		/** What it is for, in the attribute tests find it by. */
		name: string;
	}

	let { value = $bindable(), id, disabled = false, invalid = false, name }: Props = $props();
	let shown = $state(false);
</script>

<div class="field password-field" class:password-field-invalid={invalid} class:opacity-50={disabled}>
	<input
		{id}
		type={shown ? 'text' : 'password'}
		class="min-w-0 flex-1 bg-transparent"
		bind:value
		{disabled}
		autocomplete="new-password"
		spellcheck="false"
		aria-invalid={invalid}
		data-password={name}
	/>
	<button
		type="button"
		class="icon-button field-button"
		{disabled}
		aria-label={shown ? 'Hide password' : 'Show password'}
		aria-pressed={shown}
		data-tip={shown ? 'Hide password' : 'Show password'}
		onclick={() => (shown = !shown)}
	>
		{#if shown}
			<EyeOff size={14} aria-hidden="true" />
		{:else}
			<Eye size={14} aria-hidden="true" />
		{/if}
	</button>
</div>
