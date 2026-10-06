// What the Settings dialog shows (section 6.6): groups, listed as tabs down its left side,
// each holding sections of rows, one setting per row. The dialog renders this list and
// nothing else, so a new setting is a field in Rust's `Settings` plus one row here.
//
// A row reads and writes its value on the dialog's draft (`read` and `write`), so one setting
// can show as several rows (each quick tool is a switch) and a two-value setting can show as
// a switch (the floating toolbar's visibility). Booleans, and choices between "the usual" and
// one alternative, are switches; choices between peers are dropdowns.
import { Highlighter, Palette, PanelBottom, SlidersHorizontal } from '@lucide/svelte';
import type { Component } from 'svelte';

import { APP_NAME } from '#lib/config.ts';
import { MAX_TIP_DELAY_MS } from '#lib/features/annotations/bars.ts';
import { QUICK_TOOLS } from '#lib/features/annotations/tools.ts';
import type { Appearance, QuickTool, Settings, SettingsInput, ToolbarPosition, ToolbarStyle } from '#lib/ipc/index.ts';

/** The dialog's working copy: the stored settings, with `author` empty when it is the default. */
export type Draft = Settings;

type KeyOf<V> = { [K in keyof SettingsInput]: SettingsInput[K] extends V ? K : never }[keyof SettingsInput];

interface RowBase {
	/** Unique in the dialog; the control's element id is `setting-<id>`. */
	id: string;
	/** The setting it changes, for the test that every setting has a row. */
	key: keyof SettingsInput;
	label: string;
	description?: string | ((draft: Draft) => string);
	/** Shown, but not changeable, while another setting makes it moot. */
	disabled?: (draft: Draft) => boolean;
}

export type SettingRow =
	| (RowBase & { kind: 'switch'; read: (d: Draft) => boolean; write: (d: Draft, on: boolean) => void })
	| (RowBase & {
			kind: 'choice';
			options: readonly { value: string; label: string }[];
			read: (d: Draft) => string;
			write: (d: Draft, value: string) => void;
	  })
	| (RowBase & {
			kind: 'text';
			placeholder?: (d: Draft) => string;
			maxLength: number;
			read: (d: Draft) => string;
			write: (d: Draft, value: string) => void;
	  })
	| (RowBase & {
			kind: 'range';
			min: number;
			max: number;
			step: number;
			/** The value as shown beside the slider and read out by screen readers. */
			format: (value: number) => string;
			read: (d: Draft) => number;
			write: (d: Draft, value: number) => void;
	  });

export interface SettingSection {
	/** Untitled sections run on from the group's title. */
	title?: string;
	description?: string;
	rows: SettingRow[];
}

export interface SettingGroup {
	id: string;
	label: string;
	icon: Component<{ size?: number; 'aria-hidden'?: boolean | 'true' }>;
	sections: SettingSection[];
}

type Extra = Pick<RowBase, 'description' | 'disabled'>;

/** A switch for a boolean setting. */
function toggle(key: KeyOf<boolean>, label: string, extra: Extra = {}): SettingRow {
	return { kind: 'switch', id: key, key, label, ...extra, read: (d) => d[key], write: (d, on) => (d[key] = on) };
}

/** A switch for a setting with two values: off is the usual one. */
function toggleBetween<K extends KeyOf<string>>(
	key: K,
	off: SettingsInput[K],
	on: SettingsInput[K],
	label: string,
	extra: Extra = {}
): SettingRow {
	return {
		kind: 'switch',
		id: key,
		key,
		label,
		...extra,
		read: (d) => d[key] === on,
		write: (d, value) => ((d as SettingsInput)[key] = value ? on : off)
	};
}

/** A dropdown for a setting with several values. */
function choice<K extends KeyOf<string>>(
	key: K,
	label: string,
	options: readonly { value: SettingsInput[K]; label: string }[],
	extra: Extra = {}
): SettingRow {
	return {
		kind: 'choice',
		id: key,
		key,
		label,
		options,
		...extra,
		read: (d) => d[key],
		write: (d, value) => {
			const option = options.find((o) => o.value === value);
			if (option) (d as SettingsInput)[key] = option.value;
		}
	};
}

/** A switch per quick tool, writing the chosen ones in the bar's order. */
function quickTool(tool: QuickTool, label: string): SettingRow {
	return {
		kind: 'switch',
		id: `quickTools-${tool}`,
		key: 'quickTools',
		label,
		read: (d) => d.quickTools.includes(tool),
		write: (d, on) => {
			const chosen = new Set(d.quickTools);
			if (on) chosen.add(tool);
			else chosen.delete(tool);
			d.quickTools = QUICK_TOOLS.map((t) => t.id).filter((id) => chosen.has(id));
		}
	};
}

const docked = (d: Draft) => d.toolbarStyle === 'panel';

export const SETTING_GROUPS: readonly SettingGroup[] = [
	{
		id: 'general',
		label: 'General',
		icon: SlidersHorizontal,
		sections: [
			{
				rows: [
					{
						kind: 'text',
						id: 'author',
						key: 'author',
						label: 'Author name',
						description: (d) =>
							`Shown on annotations you add. Leave it empty to use your Windows user name (${d.defaultAuthor}).`,
						placeholder: (d) => d.defaultAuthor,
						maxLength: 200,
						read: (d) => d.author,
						write: (d, value) => (d.author = value)
					},
					toggle('checkForUpdates', `Check for updates when ${APP_NAME} starts`, {
						description: `${APP_NAME} asks its GitHub page for the latest version. Nothing about you or your files is sent.`
					})
				]
			}
		]
	},
	{
		id: 'appearance',
		label: 'Appearance',
		icon: Palette,
		sections: [
			{
				rows: [
					choice('appearance', 'Theme', [
						{ value: 'system', label: 'Use system setting' },
						{ value: 'light', label: 'Light' },
						{ value: 'dark', label: 'Dark' }
					] satisfies { value: Appearance; label: string }[])
				]
			},
			{
				title: 'Motion',
				description: 'Turned off whenever Windows is set to show fewer animations.',
				rows: [
					toggle('smoothZoom', 'Smooth zooming', {
						description: 'Zooming glides to the new size instead of jumping. A touchpad pinch always follows your fingers.'
					}),
					toggle('smoothAnnotationScroll', 'Smooth scrolling to annotations', {
						description: 'Picking an annotation in the list glides the page to it instead of jumping.'
					})
				]
			}
		]
	},
	{
		id: 'annotations',
		label: 'Annotations',
		icon: Highlighter,
		sections: [
			{
				rows: [
					toggle('openCommentAfterMarkup', 'Add a comment after marking text', {
						description: 'A new highlight, underline, strikeout or squiggly opens its comment beside it, ready to type.'
					}),
					toggle('rememberAnnotationStyle', 'Use the last colour and opacity for new annotations', {
						description: `Each type keeps the colour you last gave one, even after ${APP_NAME} restarts. When off, new annotations start from the default colours each time.`
					}),
					{
						kind: 'range',
						id: 'tooltipDelayMs',
						key: 'tooltipDelayMs',
						label: 'Comment tooltip delay',
						description: 'How long the pointer rests on an annotation before its comment shows.',
						min: 0,
						max: MAX_TIP_DELAY_MS,
						step: 100,
						format: (ms) => `${(ms / 1000).toFixed(1)} s`,
						read: (d) => d.tooltipDelayMs,
						write: (d, ms) => (d.tooltipDelayMs = ms)
					}
				]
			}
		]
	},
	{
		id: 'toolbars',
		label: 'Toolbars',
		icon: PanelBottom,
		sections: [
			{
				title: 'Annotation toolbar',
				rows: [
					choice('toolbarStyle', 'Placement', [
						{ value: 'floating', label: 'Floating over the pages' },
						{ value: 'panel', label: 'In the bar above the pages' }
					] satisfies { value: ToolbarStyle; label: string }[]),
					choice(
						'toolbarPosition',
						'Where it floats',
						[
							{ value: 'bottom', label: 'Bottom' },
							{ value: 'top', label: 'Top' }
						] satisfies { value: ToolbarPosition; label: string }[],
						{ disabled: docked }
					),
					toggleBetween('toolbarVisibility', 'always', 'onHover', 'Show only when the pointer is near', {
						description:
							'The floating toolbar stays hidden until the pointer comes close to its edge. It also shows while it has keyboard focus, and briefly after you pick a tool.',
						disabled: docked
					})
				]
			},
			{
				title: 'Quick tools for selected text',
				description: 'Buttons over text you select with the Select tool. With none chosen, no bar appears.',
				rows: QUICK_TOOLS.map((t) => quickTool(t.id, t.label))
			}
		]
	}
];

/** What `setSettings` takes, from the draft. */
export function toInput(d: Draft): SettingsInput {
	return {
		author: d.author.trim(),
		appearance: d.appearance,
		toolbarStyle: d.toolbarStyle,
		toolbarPosition: d.toolbarPosition,
		toolbarVisibility: d.toolbarVisibility,
		quickTools: [...d.quickTools],
		checkForUpdates: d.checkForUpdates,
		smoothZoom: d.smoothZoom,
		smoothAnnotationScroll: d.smoothAnnotationScroll,
		tooltipDelayMs: Number(d.tooltipDelayMs),
		openCommentAfterMarkup: d.openCommentAfterMarkup,
		rememberAnnotationStyle: d.rememberAnnotationStyle
	};
}

/** The draft for stored settings: the author left empty when it is the Windows user name. */
export function toDraft(s: Settings): Draft {
	return { ...s, quickTools: [...s.quickTools], author: s.author === s.defaultAuthor ? '' : s.author };
}
