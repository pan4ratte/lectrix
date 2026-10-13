// Puts settings from Rust into effect (startup, a change in Settings, a reset): the store
// the rest of the app reads, the theme, the annotation tools' memory and preset colours,
// the visual adjustments every document is shown with, and the shortcuts.

import { tools } from '#lib/features/annotations/state.svelte.ts';
import { pageDefaults } from '#lib/features/viewer/adjustments.svelte.ts';
import { normalizeAdjustments } from '#lib/features/viewer/adjustments.ts';
import type { Settings } from '#lib/ipc/index.ts';
import { setShortcuts } from '#lib/shortcuts.ts';
import { app } from '#lib/stores/app.svelte.ts';
import { applyAppearance } from '#lib/theme.ts';

export function applySettings(s: Settings) {
	app.settings = s;
	applyAppearance(s.appearance);
	tools.setRemember(s.rememberAnnotationStyle);
	tools.setPresets(s.presetColors);
	pageDefaults.value = normalizeAdjustments(s.pageAdjustments);
	setShortcuts(s.shortcuts);
}
