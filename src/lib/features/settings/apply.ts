// Puts settings from Rust into effect (startup, a change in Settings, a reset): the store
// the rest of the app reads, the theme, the annotation tools' memory and preset colours,
// the visual adjustments every document is shown with, and the shortcuts. Also the theme
// switched from the keyboard.

import { tools } from '#lib/features/annotations/state.svelte.ts';
import { pageDefaults } from '#lib/features/viewer/adjustments.svelte.ts';
import { normalizeAdjustments } from '#lib/features/viewer/adjustments.ts';
import { setSettings, toAppError, type Settings } from '#lib/ipc/index.ts';
import { setShortcuts } from '#lib/shortcuts.ts';
import { app } from '#lib/stores/app.svelte.ts';
import { applyAppearance } from '#lib/theme.ts';

import { toDraft, toInput } from './schema.ts';

export function applySettings(s: Settings) {
	app.settings = s;
	applyAppearance(s.appearance);
	tools.setRemember(s.rememberAnnotationStyle);
	tools.setPresets(s.presetColors);
	pageDefaults.value = normalizeAdjustments(s.pageAdjustments);
	setShortcuts(s.shortcuts);
}

/** Switches from the theme on screen to the other one, light or dark (a command without a
 * default shortcut). It is Settings' Appearance, stored as a choice made there is, so a
 * theme that followed the system no longer does until System is chosen again. */
export async function toggleTheme() {
	const s = app.settings;
	if (!s) return;
	const dark = s.appearance === 'dark' || (s.appearance === 'system' && matchMedia('(prefers-color-scheme: dark)').matches);
	try {
		applySettings(await setSettings(toInput({ ...toDraft(s), appearance: dark ? 'light' : 'dark' })));
	} catch (e) {
		app.showError(toAppError(e));
	}
}
