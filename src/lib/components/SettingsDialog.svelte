<script lang="ts">
	// Settings: the author name new annotations get (section 6.5), the appearance, System
	// (default), Light or Dark (section 8), the annotation toolbar's look (floating or a
	// panel above the pages) and, when floating, where it sits and when it shows, and the
	// buttons of the bar over selected text (section 6.6). Stored in app data
	// by Rust; everything applies at once.
	import { Check } from '@lucide/svelte';
	import { Checkbox, Dialog, RadioGroup } from 'bits-ui';

	import { QUICK_TOOLS } from '#lib/features/annotations/tools.ts';
	import {
		getSettings,
		setSettings,
		toAppError,
		type Appearance,
		type QuickTool,
		type ToolbarPosition,
		type ToolbarStyle,
		type ToolbarVisibility
	} from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import { applyAppearance } from '#lib/theme.ts';

	let author = $state('');
	let defaultAuthor = $state('');
	let appearance = $state<Appearance>('system');
	let toolbarStyle = $state<ToolbarStyle>('floating');
	let toolbarPosition = $state<ToolbarPosition>('bottom');
	let toolbarVisibility = $state<ToolbarVisibility>('always');
	let quickTools = $state<QuickTool[]>([]);
	let saving = $state(false);

	$effect(() => {
		if (!app.settingsOpen) return;
		void getSettings()
			.then((s) => {
				author = s.author === s.defaultAuthor ? '' : s.author;
				defaultAuthor = s.defaultAuthor;
				appearance = s.appearance;
				toolbarStyle = s.toolbarStyle;
				toolbarPosition = s.toolbarPosition;
				toolbarVisibility = s.toolbarVisibility;
				quickTools = s.quickTools;
			})
			.catch((e: unknown) => app.showError(toAppError(e)));
	});

	function toggleQuickTool(tool: QuickTool, on: boolean) {
		const chosen = new Set(quickTools);
		if (on) chosen.add(tool);
		else chosen.delete(tool);
		quickTools = QUICK_TOOLS.map((t) => t.id).filter((id) => chosen.has(id));
	}

	async function save(event: SubmitEvent) {
		event.preventDefault();
		saving = true;
		try {
			const s = await setSettings({
				author: author.trim(),
				appearance,
				toolbarStyle,
				toolbarPosition,
				toolbarVisibility,
				quickTools
			});
			app.settings = s;
			applyAppearance(s.appearance);
			app.settingsOpen = false;
		} catch (e) {
			app.showError(toAppError(e));
		} finally {
			saving = false;
		}
	}

	const appearanceChoices: { value: Appearance; label: string }[] = [
		{ value: 'system', label: 'Use system setting' },
		{ value: 'light', label: 'Light' },
		{ value: 'dark', label: 'Dark' }
	];
	const styleChoices: { value: ToolbarStyle; label: string }[] = [
		{ value: 'floating', label: 'Floating over the pages' },
		{ value: 'panel', label: 'Panel above the pages' }
	];
	const positionChoices: { value: ToolbarPosition; label: string }[] = [
		{ value: 'bottom', label: 'Bottom' },
		{ value: 'top', label: 'Top' }
	];
	const visibilityChoices: { value: ToolbarVisibility; label: string }[] = [
		{ value: 'always', label: 'Always' },
		{ value: 'onHover', label: 'When the pointer is near' }
	];
</script>

{#snippet radios<T extends string>(
	label: string,
	choices: { value: T; label: string }[],
	value: T,
	set: (v: T) => void,
	disabled = false
)}
	<fieldset class="flex flex-col gap-1" {disabled} class:text-fg-muted={disabled}>
		<legend class="mb-1 text-sm">{label}</legend>
		<RadioGroup.Root class="flex flex-col gap-1" {value} {disabled} onValueChange={(v) => set(v as T)} aria-label={label}>
			{#each choices as c (c.value)}
				<label class="flex h-7 items-center gap-2">
					<RadioGroup.Item value={c.value} class="radio" aria-label={c.label}>
						{#snippet children({ checked })}
							{#if checked}<span class="radio-dot"></span>{/if}
						{/snippet}
					</RadioGroup.Item>
					<span>{c.label}</span>
				</label>
			{/each}
		</RadioGroup.Root>
	</fieldset>
{/snippet}

<Dialog.Root bind:open={app.settingsOpen}>
	<Dialog.Portal>
		<Dialog.Overlay class="dialog-overlay" />
		<Dialog.Content class="dialog-content flex max-h-[calc(100vh-32px)] w-[480px] flex-col">
			<Dialog.Title class="text-base font-semibold">Settings</Dialog.Title>
			<form class="mt-4 flex min-h-0 flex-col gap-4" onsubmit={save}>
				<div class="-mx-1 flex min-h-0 flex-col gap-4 overflow-y-auto px-1">
					<label class="flex flex-col gap-1">
						<span class="text-sm">Author name</span>
						<input class="field h-8" bind:value={author} placeholder={defaultAuthor} maxlength="200" />
						<span class="text-xs text-fg-muted">
							Shown on annotations you add. Leave it empty to use your Windows user name ({defaultAuthor}).
						</span>
					</label>
					{@render radios('Appearance', appearanceChoices, appearance, (v) => (appearance = v))}
					{@render radios('Annotation toolbar', styleChoices, toolbarStyle, (v) => (toolbarStyle = v))}
					<div class="grid grid-cols-2 gap-4">
						{@render radios(
							'Where it floats',
							positionChoices,
							toolbarPosition,
							(v) => (toolbarPosition = v),
							toolbarStyle === 'panel'
						)}
						{@render radios(
							'Show the floating toolbar',
							visibilityChoices,
							toolbarVisibility,
							(v) => (toolbarVisibility = v),
							toolbarStyle === 'panel'
						)}
					</div>
					<fieldset class="flex flex-col gap-1">
						<legend class="mb-1 text-sm">Quick tools for selected text</legend>
						<div class="grid grid-cols-2 gap-x-4 gap-y-1">
							{#each QUICK_TOOLS as t (t.id)}
								<label class="flex h-7 items-center gap-2">
									<Checkbox.Root
										class="checkbox"
										checked={quickTools.includes(t.id)}
										onCheckedChange={(on) => toggleQuickTool(t.id, on)}
										aria-label={t.label}
									>
										{#snippet children({ checked })}
											{#if checked}<Check size={14} strokeWidth={3} aria-hidden="true" />{/if}
										{/snippet}
									</Checkbox.Root>
									<span>{t.label}</span>
								</label>
							{/each}
						</div>
						<span class="text-xs text-fg-muted">
							Shown over text you select with the Select tool. With none chosen, no bar appears.
						</span>
					</fieldset>
				</div>
				<div class="mt-2 flex justify-end gap-2">
					<button type="submit" class="button button-primary" disabled={saving}>Save</button>
					<Dialog.Close class="button">Cancel</Dialog.Close>
				</div>
			</form>
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
