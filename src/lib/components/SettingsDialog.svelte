<script lang="ts">
	// Settings (Phase 5): the author name new annotations get (section 6.5) and the
	// appearance, System (default), Light or Dark (section 8). Stored in app data by Rust;
	// the appearance applies at once.
	import { Dialog, RadioGroup } from 'bits-ui';

	import { getSettings, setSettings, toAppError, type Appearance } from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import { applyAppearance } from '#lib/theme.ts';

	let author = $state('');
	let defaultAuthor = $state('');
	let appearance = $state<Appearance>('system');
	let saving = $state(false);

	$effect(() => {
		if (!app.settingsOpen) return;
		void getSettings()
			.then((s) => {
				author = s.author === s.defaultAuthor ? '' : s.author;
				defaultAuthor = s.defaultAuthor;
				appearance = s.appearance;
			})
			.catch((e: unknown) => app.showError(toAppError(e)));
	});

	async function save(event: SubmitEvent) {
		event.preventDefault();
		saving = true;
		try {
			const s = await setSettings({ author: author.trim(), appearance });
			applyAppearance(s.appearance);
			app.settingsOpen = false;
		} catch (e) {
			app.showError(toAppError(e));
		} finally {
			saving = false;
		}
	}

	const choices: { value: Appearance; label: string }[] = [
		{ value: 'system', label: 'Use system setting' },
		{ value: 'light', label: 'Light' },
		{ value: 'dark', label: 'Dark' }
	];
</script>

<Dialog.Root bind:open={app.settingsOpen}>
	<Dialog.Portal>
		<Dialog.Overlay class="dialog-overlay" />
		<Dialog.Content class="dialog-content w-[420px]">
			<Dialog.Title class="text-base font-semibold">Settings</Dialog.Title>
			<form class="mt-4 flex flex-col gap-4" onsubmit={save}>
				<label class="flex flex-col gap-1">
					<span class="text-sm">Author name</span>
					<input class="field h-8" bind:value={author} placeholder={defaultAuthor} maxlength="200" />
					<span class="text-xs text-fg-muted">
						Shown on annotations you add. Leave it empty to use your Windows user name ({defaultAuthor}).
					</span>
				</label>
				<fieldset class="flex flex-col gap-1">
					<legend class="mb-1 text-sm">Appearance</legend>
					<RadioGroup.Root class="flex flex-col gap-1" bind:value={appearance} aria-label="Appearance">
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
				<div class="mt-2 flex justify-end gap-2">
					<button type="submit" class="button button-primary" disabled={saving}>Save</button>
					<Dialog.Close class="button">Cancel</Dialog.Close>
				</div>
			</form>
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
