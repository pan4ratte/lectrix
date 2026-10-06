<script lang="ts" generics="T extends string | number">
	// A dropdown list in place of the native <select>, whose list the webview draws in the
	// system's colours: a button showing the choice, opening a list styled as the app's
	// menus (Bits UI Select, so arrows, typing a letter, Enter and Esc work as in a select).
	import { Check, ChevronDown } from '@lucide/svelte';
	import { Select } from 'bits-ui';

	interface Props {
		value: T;
		options: readonly { value: T; label: string }[];
		/** Called when the user picks another option (not when `value` changes from outside). */
		onchange?: (value: T) => void;
		/** The accessible name, when no <label> around the dropdown gives one. */
		label?: string;
		/** The button's id, for a <label for> elsewhere. */
		id?: string;
		describedby?: string;
		title?: string;
		disabled?: boolean;
		/** Size classes for the button (height, text size, width). */
		class?: string;
	}

	let {
		value = $bindable(),
		options,
		onchange,
		label,
		id,
		describedby,
		title,
		disabled = false,
		class: className = ''
	}: Props = $props();

	let open = $state(false);
	const current = $derived(options.find((o) => o.value === value));
	const items = $derived(options.map((o) => ({ value: String(o.value), label: o.label })));

	function pick(key: string) {
		const option = options.find((o) => String(o.value) === key);
		if (!option || option.value === value) return;
		value = option.value;
		onchange?.(option.value);
	}

	/** Esc closes the list only: it must not also pick the Select tool or close a panel. */
	function onContentKey(event: KeyboardEvent) {
		if (event.key !== 'Escape') return;
		event.preventDefault();
		event.stopPropagation();
		open = false;
	}
</script>

<Select.Root type="single" bind:open value={String(value)} onValueChange={pick} {items} {disabled}>
	<Select.Trigger class="dropdown {className}" {id} aria-label={label} aria-describedby={describedby} {title}>
		<span class="min-w-0 flex-1 truncate text-left">{current?.label ?? ''}</span>
		<ChevronDown size={14} aria-hidden="true" class="shrink-0 text-fg-muted" />
	</Select.Trigger>
	<Select.Portal>
		<Select.Content
			class="menu-content dropdown-list"
			side="bottom"
			align="start"
			sideOffset={4}
			escapeKeydownBehavior="ignore"
			onkeydown={onContentKey}
		>
			<Select.Viewport>
				{#each options as option (option.value)}
					<Select.Item class="menu-item dropdown-item" value={String(option.value)} label={option.label}>
						{#snippet children({ selected })}
							<span class="min-w-0 flex-1 truncate">{option.label}</span>
							{#if selected}
								<Check size={14} aria-hidden="true" class="shrink-0" />
							{/if}
						{/snippet}
					</Select.Item>
				{/each}
			</Select.Viewport>
		</Select.Content>
	</Select.Portal>
</Select.Root>
