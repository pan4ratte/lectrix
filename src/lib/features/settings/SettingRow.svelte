<script lang="ts">
	// One setting: its name and description at the left, its control at the right (a switch,
	// a dropdown, a text field or a slider, by the row's kind in schema.ts). The name labels
	// the control, so clicking it flips a switch or focuses the field. Each change goes to
	// `commit`: switches and dropdowns at once, fields and sliders once they pause or are left.
	import Dropdown from '#lib/components/Dropdown.svelte';
	import Switch from '#lib/components/Switch.svelte';

	import type { Draft, SettingRow } from './schema.ts';

	interface Props {
		row: SettingRow;
		draft: Draft;
		/** Stores the draft, after `delay` ms without another change. */
		commit: (delay?: number) => void;
	}

	let { row, draft, commit }: Props = $props();

	/** How long typing pauses before the text is stored. */
	const TYPING_PAUSE_MS = 500;

	const id = $derived(`setting-${row.id}`);
	const noteId = $derived(`${id}-note`);
	const description = $derived(typeof row.description === 'function' ? row.description(draft) : row.description);
	const disabled = $derived(row.disabled?.(draft) ?? false);
</script>

<div class="setting-row" data-disabled={disabled ? '' : undefined}>
	<div class="flex min-w-0 flex-1 flex-col gap-0.5">
		<label class="setting-label" for={id}>{row.label}</label>
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
				<input
					{id}
					{disabled}
					type="range"
					class="settings-slider"
					min={row.min}
					max={row.max}
					step={row.step}
					value={row.read(draft)}
					oninput={(e) => row.write(draft, Number(e.currentTarget.value))}
					onchange={() => commit()}
					aria-valuetext={text}
					aria-describedby={description ? noteId : undefined}
				/>
				<span class="w-10 shrink-0 text-right text-[13px] tabular-nums" aria-hidden="true">{text}</span>
			</div>
		{/if}
	</div>
</div>
