<script lang="ts">
	// An on/off switch, as Windows 11's toggle: Bits UI Switch (a button with role="switch",
	// Space and Enter flip it). Its name comes from a <label for={id}> or `label`.
	import { Switch } from 'bits-ui';

	interface Props {
		checked: boolean;
		onchange?: (on: boolean) => void;
		id?: string;
		/** The accessible name, when no <label> gives one. */
		label?: string;
		describedby?: string;
		disabled?: boolean;
	}

	let { checked = $bindable(), onchange, id, label, describedby, disabled = false }: Props = $props();
</script>

<Switch.Root
	class="switch"
	{id}
	{disabled}
	{checked}
	onCheckedChange={(on) => {
		checked = on;
		onchange?.(on);
	}}
	aria-label={label}
	aria-describedby={describedby}
>
	<Switch.Thumb class="switch-thumb" />
</Switch.Root>
