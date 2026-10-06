<script module lang="ts">
	// The group shown last, shown again when Settings reopens while the app runs.
	let lastGroup = 'general';
</script>

<script lang="ts">
	// Settings (section 6.6): the groups of schema.ts as tabs down the left side, the chosen
	// group's settings at the right, one per row. Every change is stored in app data through
	// Rust and applies at once; there is no Save button. Typing in a field is stored once
	// the typing pauses, or when the field is left.
	import { X } from '@lucide/svelte';
	import { Dialog, Tabs } from 'bits-ui';
	import { untrack } from 'svelte';

	import { tools } from '#lib/features/annotations/state.svelte.ts';
	import { getSettings, setSettings, toAppError } from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import { applyAppearance } from '#lib/theme.ts';

	import SettingRow from './SettingRow.svelte';
	import { SETTING_GROUPS, toDraft, toInput, type Draft } from './schema.ts';

	let draft = $state<Draft | null>(null);
	let group = $state(lastGroup);
	let pending: ReturnType<typeof setTimeout> | undefined;
	/** Stores run one after another, so the last change is the one stored. */
	let stored = Promise.resolve();

	$effect(() => {
		lastGroup = group;
	});

	$effect(() => {
		if (!app.settingsOpen) {
			untrack(() => {
				if (pending !== undefined) commit();
				draft = null;
			});
			return;
		}
		void getSettings()
			.then((s) => (draft = toDraft(s)))
			.catch((e: unknown) => app.showError(toAppError(e)));
	});

	/** Stores the draft and applies it, now or after `delay` ms without another change. */
	function commit(delay = 0) {
		clearTimeout(pending);
		pending = undefined;
		if (delay > 0) {
			pending = setTimeout(() => commit(), delay);
			return;
		}
		if (!draft) return;
		const input = toInput(draft);
		stored = stored.then(async () => {
			try {
				const s = await setSettings(input);
				app.settings = s;
				applyAppearance(s.appearance);
				tools.setRemember(s.rememberAnnotationStyle);
			} catch (e) {
				app.showError(toAppError(e));
			}
		});
	}
</script>

<Dialog.Root bind:open={app.settingsOpen}>
	<Dialog.Portal>
		<Dialog.Overlay class="dialog-overlay" />
		<Dialog.Content class="dialog-content settings-dialog">
			<Tabs.Root bind:value={group} orientation="vertical" class="flex min-h-0 min-w-0 flex-1">
				<div class="settings-nav">
					<Dialog.Title class="settings-title">Settings</Dialog.Title>
					<Tabs.List class="flex flex-col gap-0.5" aria-label="Settings groups">
						{#each SETTING_GROUPS as g (g.id)}
							<Tabs.Trigger value={g.id} class="settings-tab">
								<g.icon size={16} aria-hidden="true" />
								<span class="truncate">{g.label}</span>
							</Tabs.Trigger>
						{/each}
					</Tabs.List>
				</div>
				{#each SETTING_GROUPS as g (g.id)}
					<Tabs.Content value={g.id} class="settings-page">
						<h2 class="settings-heading">{g.label}</h2>
						{#if draft}
							{#each g.sections as section, i (i)}
								<section class="settings-section" aria-label={section.title}>
									{#if section.title}
										<h3 class="settings-section-title">{section.title}</h3>
									{/if}
									{#if section.description}
										<p class="setting-note">{section.description}</p>
									{/if}
									<div class="flex flex-col">
										{#each section.rows as row (row.id)}
											<SettingRow {row} {draft} {commit} />
										{/each}
									</div>
								</section>
							{/each}
						{/if}
					</Tabs.Content>
				{/each}
			</Tabs.Root>
			<Dialog.Close class="icon-button dialog-close" aria-label="Close">
				<X size={16} aria-hidden="true" />
			</Dialog.Close>
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
