<script lang="ts">
	// One setting: its name (after its icon, where it has one) and description at the left,
	// its control at the right (a switch, a dropdown, a pill of segments, a text field, a
	// slider, the preset colours or a button, by the row's kind in schema.ts). The name labels
	// the control, so clicking it flips a switch or focuses the field. Each change goes to `commit`: switches, dropdowns and segments at once, fields
	// and sliders once they pause or are left. A button goes to `onaction`.
	import Dropdown from '#lib/components/Dropdown.svelte';
	import Segmented from '#lib/components/Segmented.svelte';
	import Slider from '#lib/components/Slider.svelte';
	import Switch from '#lib/components/Switch.svelte';

	import PresetColorsField from './PresetColorsField.svelte';
	import type { Draft, SettingRow } from './schema.ts';

	interface Props {
		row: SettingRow;
		draft: Draft;
		/** Stores the draft, after `delay` ms without another change. */
		commit: (delay?: number) => void;
		/** A row's button was pressed. */
		onaction: (id: string) => void;
	}

	let { row, draft, commit, onaction }: Props = $props();

	/** How long typing pauses before the text is stored. */
	const TYPING_PAUSE_MS = 500;

	const id = $derived(`setting-${row.id}`);
	const labelId = $derived(`${id}-label`);
	const noteId = $derived(`${id}-note`);
	const description = $derived(typeof row.description === 'function' ? row.description(draft) : row.description);
	const disabled = $derived(row.disabled?.(draft) ?? false);
</script>

<div class="setting-row" data-disabled={disabled ? '' : undefined}>
	<div class="flex min-w-0 flex-1 flex-col gap-0.5">
		<label id={labelId} class="setting-label" for={id}>
			{#if row.icon}
				<span class="setting-label-icon"><row.icon size={16} aria-hidden="true" /></span>
			{/if}
			{row.label}
		</label>
		{#if description}
			<span id={noteId} class="setting-note">{description}</span>
		{/if}
	</div>
	<div class="flex shrink-0 items-center">
		{#if row.kind === 'switch'}
			<Switch
				{id}
				{disabled}
				checked={row.read(draft)}
				onchange={(on) => {
					row.write(draft, on);
					commit();
				}}
				describedby={description ? noteId : undefined}
			/>
		{:else if row.kind === 'choice'}
			<Dropdown
				{id}
				{disabled}
				class="h-8 w-[208px] text-[13px]"
				value={row.read(draft)}
				options={row.options}
				onchange={(v) => {
					row.write(draft, v);
					commit();
				}}
				describedby={description ? noteId : undefined}
			/>
		{:else if row.kind === 'segments'}
			<Segmented
				{id}
				{disabled}
				value={row.read(draft)}
				options={row.options}
				onchange={(v) => {
					row.write(draft, v);
					commit();
				}}
				labelledby={labelId}
				describedby={description ? noteId : undefined}
			/>
		{:else if row.kind === 'text'}
			<input
				{id}
				{disabled}
				class="field h-8 w-[208px] text-[13px]"
				value={row.read(draft)}
				oninput={(e) => {
					row.write(draft, e.currentTarget.value);
					commit(TYPING_PAUSE_MS);
				}}
				onchange={() => commit()}
				placeholder={row.placeholder?.(draft)}
				maxlength={row.maxLength}
				aria-describedby={description ? noteId : undefined}
			/>
		{:else if row.kind === 'range'}
			{@const text = row.format(row.read(draft))}
			<div class="flex w-[208px] items-center gap-3">
				<Slider
					{id}
					{disabled}
					min={row.min}
					max={row.max}
					step={row.step}
					value={row.read(draft)}
					oninput={(v) => row.write(draft, v)}
					onchange={() => commit()}
					valuetext={text}
					describedby={description ? noteId : undefined}
				/>
				<span class="w-10 shrink-0 text-right text-[13px] tabular-nums" aria-hidden="true">{text}</span>
			</div>
		{:else if row.kind === 'presets'}
			<PresetColorsField
				{id}
				labelledby={labelId}
				describedby={description ? noteId : undefined}
				value={row.read(draft)}
				onchange={(v) => {
					row.write(draft, v);
					commit();
				}}
			/>
		{:else if row.kind === 'action'}
			<button
				{id}
				{disabled}
				type="button"
				class="button"
				class:button-danger={row.danger}
				aria-describedby={description ? noteId : undefined}
				onclick={() => onaction(row.id)}
			>
				{row.button}
			</button>
		{/if}
	</div>
</div>
