<script module lang="ts">
	// The group shown last, shown again when Settings reopens while the app runs.
	let lastGroup = 'general';
</script>

<script lang="ts">
	// Settings (section 6.6): the groups of schema.ts as tabs down the left side, the chosen
	// group's settings at the right, one per row (the shortcuts have a panel of their own).
	// Every change is stored in app data through Rust and applies at once; there is no Save
	// button. Typing in a field is stored once the typing pauses, or when the field is left.
	import { X } from '@lucide/svelte';
	import { Dialog, Tabs } from 'bits-ui';
	import { untrack } from 'svelte';

	import { toolMark } from '#lib/features/annotations/toolMark.ts';
	import { NO_ADJUSTMENTS } from '#lib/features/viewer/adjustments.ts';
	import { getSettings, resetSettings, setSettings, toAppError } from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	import { applySettings } from './apply.ts';
	import SettingRow from './SettingRow.svelte';
	import { RESET_ADJUSTMENTS_ROW, RESET_COLORS_ROW, RESET_ROW, SETTING_GROUPS, toDraft, toInput, type Draft } from './schema.ts';
	import ShortcutsPanel from './ShortcutsPanel.svelte';

	let draft = $state<Draft | null>(null);
	let group = $state(lastGroup);
	let pending: ReturnType<typeof setTimeout> | undefined;
	/** Stores run one after another, so the last change is the one stored. */
	let stored = Promise.resolve();
	/** Changes on their way to Rust: until the last of them is back, `app.settings` is not
	 * yet what the draft says. */
	let storing = 0;
	/** A shortcut is being pressed: Esc leaves that, not the dialog. */
	let capturing = $state(false);

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

	// The theme switched from the keyboard while the dialog is open: Appearance shows it.
	$effect(() => {
		const appearance = app.settings?.appearance;
		untrack(() => {
			if (draft && appearance && storing === 0) draft.appearance = appearance;
		});
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
		storing++;
		stored = stored.then(async () => {
			try {
				const s = await setSettings(input);
				storing--;
				applySettings(s);
			} catch (e) {
				storing--;
				app.showError(toAppError(e));
			}
		});
	}

	/** Restores the defaults, after asking: every setting, the shortcuts included. */
	async function restoreDefaults() {
		const choice = await app.ask({
			title: 'Restore default settings?',
			message: 'Every setting goes back to its default, your keyboard shortcuts and author name included.',
			detail: 'This can’t be undone.',
			buttons: [
				{ id: 'reset', label: 'Restore defaults', danger: true },
				{ id: 'cancel', label: 'Cancel' }
			],
			cancel: 'cancel'
		});
		if (choice !== 'reset') return;
		// A change still waiting to be stored is dropped with the rest.
		clearTimeout(pending);
		pending = undefined;
		stored = stored.then(async () => {
			try {
				const s = await resetSettings();
				applySettings(s);
				if (draft) draft = toDraft(s);
			} catch (e) {
				app.showError(toAppError(e));
			}
		});
	}

	function onAction(id: string) {
		if (id === RESET_ROW) void restoreDefaults();
		else if (id === RESET_COLORS_ROW && draft) {
			draft.presetColors = {};
			commit();
		} else if (id === RESET_ADJUSTMENTS_ROW && draft) {
			draft.pageAdjustments = { ...NO_ADJUSTMENTS };
			commit();
		}
	}
</script>

<Dialog.Root bind:open={app.settingsOpen}>
	<Dialog.Portal>
		<Dialog.Overlay class="dialog-overlay" />
		<Dialog.Content class="dialog-content settings-dialog" escapeKeydownBehavior={capturing ? 'ignore' : 'close'}>
			<Tabs.Root bind:value={group} orientation="vertical" class="flex min-h-0 min-w-0 flex-1">
				<div class="settings-nav">
					<Dialog.Title class="settings-title">Settings</Dialog.Title>
					<!-- The chosen group's fill is a mark that glides to the tab picked. -->
					<Tabs.List class="tool-group flex flex-col gap-0.5" aria-label="Settings groups" {@attach toolMark}>
						<span class="tool-mark settings-mark" aria-hidden="true"></span>
						{#each SETTING_GROUPS as g (g.id)}
							<Tabs.Trigger value={g.id} class="settings-tab glide-item">
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
							{#if g.panel?.kind === 'shortcuts'}
								<ShortcutsPanel {draft} {commit} bind:capturing />
							{/if}
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
											<SettingRow {row} {draft} {commit} onaction={onAction} />
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
