<script lang="ts">
	// Settings: the author name new annotations get (section 6.5), the appearance, System
	// (default), Light or Dark (section 8), smooth zooming (section 6.1), scrolling to
	// annotations and the comment tooltip's delay (section 6.5), the annotation toolbar's
	// look (floating, or docked in the bar above the pages) and, when floating, where it
	// sits and when it shows, the buttons of the bar over selected text (section 6.6), and
	// whether new text markup opens its comment and whether new annotations take the last
	// colour and opacity (section 6.5), and whether Lectrix looks for
	// updates when it starts (ADR 0011). Stored in app data by
	// Rust; everything applies at once.
	import { Check } from '@lucide/svelte';
	import { Checkbox, Dialog, RadioGroup } from 'bits-ui';

	import { APP_NAME } from '#lib/config.ts';
	import { DEFAULT_TIP_DELAY_MS, MAX_TIP_DELAY_MS } from '#lib/features/annotations/bars.ts';
	import { tools } from '#lib/features/annotations/state.svelte.ts';
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
	let checkForUpdates = $state(true);
	let smoothZoom = $state(true);
	let smoothAnnotationScroll = $state(true);
	let tooltipDelayMs = $state(DEFAULT_TIP_DELAY_MS);
	let openCommentAfterMarkup = $state(false);
	let rememberAnnotationStyle = $state(true);
	const delayText = $derived(`${(tooltipDelayMs / 1000).toFixed(1)} s`);
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
				checkForUpdates = s.checkForUpdates;
				smoothZoom = s.smoothZoom;
				smoothAnnotationScroll = s.smoothAnnotationScroll;
				tooltipDelayMs = s.tooltipDelayMs;
				openCommentAfterMarkup = s.openCommentAfterMarkup;
				rememberAnnotationStyle = s.rememberAnnotationStyle;
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
				quickTools,
				checkForUpdates,
				smoothZoom,
				smoothAnnotationScroll,
				tooltipDelayMs: Number(tooltipDelayMs),
				openCommentAfterMarkup,
				rememberAnnotationStyle
			});
			app.settings = s;
			applyAppearance(s.appearance);
			tools.setRemember(s.rememberAnnotationStyle);
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
		{ value: 'panel', label: 'In the bar above the pages' }
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
					<div class="flex flex-col gap-1">
						<label class="flex h-7 items-center gap-2">
							<Checkbox.Root
								class="checkbox"
								checked={smoothZoom}
								onCheckedChange={(on) => (smoothZoom = on)}
								aria-label="Smooth zooming"
								aria-describedby="settings-zoom-note"
							>
								{#snippet children({ checked })}
									{#if checked}<Check size={14} strokeWidth={3} aria-hidden="true" />{/if}
								{/snippet}
							</Checkbox.Root>
							<span>Smooth zooming</span>
						</label>
						<span id="settings-zoom-note" class="text-xs text-fg-muted">
							Zooming glides to the new size instead of jumping. A touchpad pinch always follows your fingers.
						</span>
					</div>
					<div class="flex flex-col gap-1">
						<label class="flex h-7 items-center gap-2">
							<Checkbox.Root
								class="checkbox"
								checked={smoothAnnotationScroll}
								onCheckedChange={(on) => (smoothAnnotationScroll = on)}
								aria-label="Smooth scrolling to annotations"
								aria-describedby="settings-annotation-scroll-note"
							>
								{#snippet children({ checked })}
									{#if checked}<Check size={14} strokeWidth={3} aria-hidden="true" />{/if}
								{/snippet}
							</Checkbox.Root>
							<span>Smooth scrolling to annotations</span>
						</label>
						<span id="settings-annotation-scroll-note" class="text-xs text-fg-muted">
							Picking an annotation in the list glides the page to it instead of jumping.
						</span>
					</div>
					<div class="flex flex-col gap-1">
						<label class="text-sm" for="settings-tooltip-delay">Comment tooltip delay</label>
						<div class="flex items-center gap-3">
							<input
								id="settings-tooltip-delay"
								type="range"
								class="settings-slider"
								min="0"
								max={MAX_TIP_DELAY_MS}
								step="100"
								bind:value={tooltipDelayMs}
								aria-valuetext={delayText}
								aria-describedby="settings-tooltip-delay-note"
							/>
							<span class="w-12 shrink-0 text-right text-sm tabular-nums" aria-hidden="true">{delayText}</span>
						</div>
						<span id="settings-tooltip-delay-note" class="text-xs text-fg-muted">
							How long the pointer rests on an annotation before its comment shows.
						</span>
					</div>
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
					<div class="flex flex-col gap-1">
						<label class="flex h-7 items-center gap-2">
							<Checkbox.Root
								class="checkbox"
								checked={openCommentAfterMarkup}
								onCheckedChange={(on) => (openCommentAfterMarkup = on)}
								aria-label="Add a comment after marking text"
								aria-describedby="settings-markup-comment-note"
							>
								{#snippet children({ checked })}
									{#if checked}<Check size={14} strokeWidth={3} aria-hidden="true" />{/if}
								{/snippet}
							</Checkbox.Root>
							<span>Add a comment after marking text</span>
						</label>
						<span id="settings-markup-comment-note" class="text-xs text-fg-muted">
							A new highlight, underline, strikeout or squiggly opens its comment beside it, ready to type.
						</span>
					</div>
					<div class="flex flex-col gap-1">
						<label class="flex h-7 items-center gap-2">
							<Checkbox.Root
								class="checkbox"
								checked={rememberAnnotationStyle}
								onCheckedChange={(on) => (rememberAnnotationStyle = on)}
								aria-label="Use the last colour and opacity for new annotations"
								aria-describedby="settings-remember-style-note"
							>
								{#snippet children({ checked })}
									{#if checked}<Check size={14} strokeWidth={3} aria-hidden="true" />{/if}
								{/snippet}
							</Checkbox.Root>
							<span>Use the last colour and opacity for new annotations</span>
						</label>
						<span id="settings-remember-style-note" class="text-xs text-fg-muted">
							Each type keeps the colour you last gave one, even after {APP_NAME} restarts. When off, new
							annotations start from the default colours each time.
						</span>
					</div>
					<div class="flex flex-col gap-1">
						<label class="flex h-7 items-center gap-2">
							<Checkbox.Root
								class="checkbox"
								checked={checkForUpdates}
								onCheckedChange={(on) => (checkForUpdates = on)}
								aria-label="Check for updates when {APP_NAME} starts"
								aria-describedby="settings-updates-note"
							>
								{#snippet children({ checked })}
									{#if checked}<Check size={14} strokeWidth={3} aria-hidden="true" />{/if}
								{/snippet}
							</Checkbox.Root>
							<span>Check for updates when {APP_NAME} starts</span>
						</label>
						<span id="settings-updates-note" class="text-xs text-fg-muted">
							{APP_NAME} asks its GitHub page for the latest version. Nothing about you or your files is sent.
						</span>
					</div>
				</div>
				<div class="mt-2 flex justify-end gap-2">
					<button type="submit" class="button button-primary" disabled={saving}>Save</button>
					<Dialog.Close class="button">Cancel</Dialog.Close>
				</div>
			</form>
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
