// What the Settings dialog shows (section 6.6): groups, listed as tabs down its left side,
// each holding sections of rows, one setting per row. The dialog renders this list and
// nothing else, so a new setting is a field in Rust's `Settings` plus one row here.
//
// A row reads and writes its value on the dialog's draft (`read` and `write`), so one setting
// can show as several rows (each quick tool is a switch) and a two-value setting can show as
// a switch (the floating toolbar's visibility). Booleans, and choices between "the usual" and
// one alternative, are switches; choices between peers are dropdowns. The colour panel's
// presets are a row of their own kind.
import { Highlighter, Keyboard, Monitor, Moon, Palette, PanelBottom, SlidersHorizontal, Sun } from '@lucide/svelte';
import type { Component } from 'svelte';

import { APP_NAME } from '#lib/config.ts';
import { MAX_TIP_DELAY_MS } from '#lib/features/annotations/bars.ts';
import { QUICK_TOOLS } from '#lib/features/annotations/tools.ts';
import { ADJUST_MAX, ADJUST_MIN, ADJUST_STEP, SEPIA_MAX, isAdjusted } from '#lib/features/viewer/adjustments.ts';
import type {
	Appearance,
	PageAdjustments,
	QuickTool,
	ScrollMode,
	Settings,
	SettingsInput,
	ToolbarDock,
	ToolbarPosition,
	ToolbarStyle
} from '#lib/ipc/index.ts';

/** The dialog's working copy: the stored settings, with `author` empty when it is the default. */
export type Draft = Settings;

type KeyOf<V> = { [K in keyof SettingsInput]: SettingsInput[K] extends V ? K : never }[keyof SettingsInput];

type Icon = Component<{ size?: number; 'aria-hidden'?: boolean | 'true' }>;

interface RowText {
	/** Unique in the dialog; the control's element id is `setting-<id>`. */
	id: string;
	label: string;
	description?: string | ((draft: Draft) => string);
	/** Shown, but not changeable, while another setting makes it moot. */
	disabled?: (draft: Draft) => boolean;
}

interface RowBase extends RowText {
	/** The setting it changes, for the test that every setting has a row. */
	key: keyof SettingsInput;
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
			/** A few peers, all shown: one pill with a segment each. */
			kind: 'segments';
			options: readonly { value: string; label: string; icon?: Icon }[];
			read: (d: Draft) => string;
			write: (d: Draft, value: string) => void;
	  })
	| (RowText & {
			/** Something to do, not a value: a button, which the dialog acts on by the row's id. */
			kind: 'action';
			button: string;
			/** It can't be undone: the button is in the destructive colour. */
			danger?: boolean;
	  })
	| (RowBase & {
			kind: 'text';
			placeholder?: (d: Draft) => string;
			maxLength: number;
			read: (d: Draft) => string;
			write: (d: Draft, value: string) => void;
	  })
	| (RowBase & {
			/** The colour panel's presets, each changeable. */
			kind: 'presets';
			read: (d: Draft) => Readonly<Record<number, string>>;
			write: (d: Draft, value: Record<number, string>) => void;
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
	icon: Icon;
	sections: SettingSection[];
	/** A group the dialog shows with a panel of its own instead of rows, and the setting
	 * that panel changes. */
	panel?: { kind: 'shortcuts'; key: keyof SettingsInput };
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

/** One pill with a segment per value, for a setting with a few. */
function segments<K extends KeyOf<string>>(
	key: K,
	label: string,
	options: readonly { value: SettingsInput[K]; label: string; icon?: Icon }[],
	extra: Extra = {}
): SettingRow {
	return {
		kind: 'segments',
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
/** Said of the floating toolbar's options while it is docked: full screen still floats it. */
const IN_FULL_SCREEN = 'Used in full screen, where the toolbar floats.';

/** A slider for one of the visual adjustments every document is shown with. */
function adjustment(part: 'brightness' | 'contrast' | 'sepia', label: string, min: number, max: number, extra: Extra = {}): SettingRow {
	return {
		kind: 'range',
		id: `pageAdjustments-${part}`,
		key: 'pageAdjustments',
		label,
		...extra,
		min,
		max,
		step: ADJUST_STEP,
		format: (value) => `${value}%`,
		read: (d) => d.pageAdjustments[part],
		write: (d, value) => (d.pageAdjustments = { ...d.pageAdjustments, [part]: value })
	};
}

/** A switch for one of them. */
function adjustmentSwitch(part: 'invert' | 'grayscale', label: string, extra: Extra = {}): SettingRow {
	return {
		kind: 'switch',
		id: `pageAdjustments-${part}`,
		key: 'pageAdjustments',
		label,
		...extra,
		read: (d) => d.pageAdjustments[part],
		write: (d, on) => (d.pageAdjustments = { ...d.pageAdjustments, [part]: on })
	};
}

/** The id of the row that restores the defaults. */
export const RESET_ROW = 'resetSettings';
/** The id of the row that shows every document as it is again. */
export const RESET_ADJUSTMENTS_ROW = 'resetPageAdjustments';
/** The id of the row that puts the default preset colours back. */
export const RESET_COLORS_ROW = 'resetPresetColors';

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
							`Shown on annotations you add. Leave empty to use your Windows user name (${d.defaultAuthor}).`,
						placeholder: (d) => d.defaultAuthor,
						maxLength: 200,
						read: (d) => d.author,
						write: (d, value) => (d.author = value)
					},
					toggle('checkForUpdates', 'Check for updates automatically', {
						description: 'Once a day, from GitHub. Nothing about you or your files is sent.'
					})
				]
			},
			{
				title: 'Documents',
				rows: [
					choice(
						'scrollMode',
						'Page display',
						[
							{ value: 'document', label: 'As the document asks' },
							{ value: 'singlePage', label: 'Single page' },
							{ value: 'singlePageContinuous', label: 'Single page, continuous' },
							{ value: 'twoPage', label: 'Two pages' },
							{ value: 'twoPageContinuous', label: 'Two pages, continuous' }
						] satisfies { value: ScrollMode; label: string }[],
						{
							description:
								'How pages are laid out when a document opens. View > Page display changes it for the open document.'
						}
					),
					toggle('pdfaReadOnly', 'Open PDF/A files for reading only', {
						description:
							'Files made for long-term archiving open protected from changes, until you choose Enable editing.'
					})
				]
			},
			{
				title: 'Selected text',
				rows: [
					toggle('selectionHandles', 'Show handles on selected text', {
						description:
							'Handles to drag at the start and end of a selection. Without them, the ends can still be dragged.'
					})
				]
			},
			{
				title: 'Search',
				rows: [
					toggle('searchMarkers', 'Mark matches beside the scrollbar', {
						description: 'While you search a document, a mark beside its scrollbar shows where each match is.'
					})
				]
			},
			{
				title: 'Links',
				rows: [
					toggle('highlightLinks', 'Highlight the link under the pointer', {
						description: 'A link in the text is tinted while the pointer is on it. Links work either way.'
					}),
					toggle('linkPreviews', 'Show where a link leads', {
						description:
							'A tooltip on the link under the pointer names the page it goes to, or its web address. It waits as long as a comment’s tooltip (Annotations).'
					})
				]
			},
			{
				title: 'Reset',
				rows: [
					{
						kind: 'action',
						id: RESET_ROW,
						label: 'Restore default settings',
						description: 'Shortcuts and author name included. Recent files and panes stay as they are.',
						button: 'Restore defaults…',
						danger: true
					}
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
					segments(
						'appearance',
						'Theme',
						[
							{ value: 'system', label: 'System', icon: Monitor },
							{ value: 'light', label: 'Light', icon: Sun },
							{ value: 'dark', label: 'Dark', icon: Moon }
						] satisfies { value: Appearance; label: string; icon: Icon }[],
						{ description: 'System follows Windows.' }
					)
				]
			},
			{
				title: 'Motion',
				description: 'Off while animations are turned off in Windows.',
				rows: [
					toggle('smoothZoom', 'Smooth zooming', {
						description: 'Zoom glides to the new size. A touchpad pinch always follows your fingers.'
					}),
					toggle('smoothAnnotationScroll', 'Smooth scrolling to annotations', {
						description: 'The page glides to an annotation picked in the list.'
					}),
					toggle('smoothPageTurn', 'Smooth scrolling to the next and previous page', {
						description: 'The view glides there when you press the Previous page or Next page button.'
					}),
					toggle('smoothBookmarkScroll', 'Smooth scrolling to bookmarks and links', {
						description: 'The view glides to the place a bookmark or a link in the text leads to.'
					}),
					toggle('smoothSearchScroll', 'Smooth scrolling to search results', {
						description: 'The view glides to the next or previous match of a search.'
					})
				]
			},
			{
				title: 'Visual adjustments',
				description:
					'How the pages of every document look. A document you adjust by itself, with the Visual adjustments button above its pages, keeps its own. Files are never changed.',
				rows: [
					adjustment('brightness', 'Brightness', ADJUST_MIN, ADJUST_MAX),
					adjustment('contrast', 'Contrast', ADJUST_MIN, ADJUST_MAX),
					adjustment('sepia', 'Sepia', 0, SEPIA_MAX, { description: 'The tone of old paper, as in an e-book.' }),
					adjustmentSwitch('invert', 'Invert colours', { description: 'Dark pages with light text. Colours keep their hue.' }),
					adjustmentSwitch('grayscale', 'Greyscale'),
					{
						kind: 'action',
						id: RESET_ADJUSTMENTS_ROW,
						label: 'Reset visual adjustments',
						description: 'Documents are shown as they are again, except the ones adjusted by themselves.',
						button: 'Reset',
						disabled: (d) => !isAdjusted(d.pageAdjustments)
					}
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
						description: 'A new highlight, underline, strikeout or squiggly opens its comment, ready to type.'
					}),
					toggle('rememberAnnotationStyle', 'Use the last colour and opacity for new annotations', {
						description: `One colour for every tool, and an opacity for each. When off, the defaults return each time ${APP_NAME} starts.`
					}),
					toggle('rememberAnnotationSort', 'Keep the annotation list’s sort for every document', {
						description: `The order last picked in the annotation list, in every file and after ${APP_NAME} restarts. When off, each document starts sorted by page.`
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
			},
			{
				title: 'Colours',
				rows: [
					toggle('colorPanelOnHover', 'Open the colours when the pointer is on their button', {
						description: 'When off, a colour button shows its colours when you click it.'
					}),
					{
						kind: 'presets',
						id: 'presetColors',
						key: 'presetColors',
						label: 'Preset colours',
						description: 'The colours every tool offers. Click one to give it another colour.',
						read: (d) => d.presetColors,
						write: (d, value) => (d.presetColors = value)
					},
					{
						kind: 'action',
						id: RESET_COLORS_ROW,
						label: 'Default preset colours',
						description: 'Puts back the colours Lectrix comes with.',
						button: 'Restore colours',
						disabled: (d) => Object.keys(d.presetColors).length === 0
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
					choice(
						'toolbarStyle',
						'Placement',
						[
							{ value: 'panel', label: 'Docked beside the pages' },
							{ value: 'floating', label: 'Floating over the pages' }
						] satisfies { value: ToolbarStyle; label: string }[],
						{ description: 'In full screen it always floats, with the page and zoom controls.' }
					),
					choice(
						'toolbarDock',
						'Where it is docked',
						[
							{ value: 'top', label: 'Top' },
							{ value: 'left', label: 'Left' },
							{ value: 'right', label: 'Right' }
						] satisfies { value: ToolbarDock; label: string }[],
						{
							description:
								'At the top, in the bar above the pages. At a side, in a bar of its own, with the tools stacked.',
							disabled: (d) => !docked(d)
						}
					),
					choice(
						'toolbarPosition',
						'Where it floats',
						[
							{ value: 'bottom', label: 'Bottom' },
							{ value: 'top', label: 'Top' },
							{ value: 'left', label: 'Left' },
							{ value: 'right', label: 'Right' }
						] satisfies { value: ToolbarPosition; label: string }[],
						{ description: (d) => `At a side, the tools are stacked.${docked(d) ? ` ${IN_FULL_SCREEN}` : ''}` }
					),
					toggleBetween('toolbarVisibility', 'always', 'onHover', 'Show only when the pointer is near', {
						description: (d) =>
							`The toolbar also shows while it has keyboard focus, and briefly after you pick a tool.${docked(d) ? ` ${IN_FULL_SCREEN}` : ''}`
					})
				]
			},
			{
				title: 'Quick tools for selected text',
				description: 'Buttons over text you select with the Select tool.',
				rows: QUICK_TOOLS.map((t) => quickTool(t.id, t.label))
			}
		]
	},
	{
		id: 'shortcuts',
		label: 'Shortcuts',
		icon: Keyboard,
		sections: [],
		panel: { kind: 'shortcuts', key: 'shortcuts' }
	}
];

function copyShortcuts(shortcuts: Settings['shortcuts']): Settings['shortcuts'] {
	return Object.fromEntries(Object.entries(shortcuts).map(([command, keys]) => [command, [...keys]]));
}

/** What `setSettings` takes, from the draft. */
export function toInput(d: Draft): SettingsInput {
	return {
		author: d.author.trim(),
		appearance: d.appearance,
		scrollMode: d.scrollMode,
		toolbarStyle: d.toolbarStyle,
		toolbarDock: d.toolbarDock,
		toolbarPosition: d.toolbarPosition,
		toolbarVisibility: d.toolbarVisibility,
		quickTools: [...d.quickTools],
		checkForUpdates: d.checkForUpdates,
		smoothZoom: d.smoothZoom,
		smoothAnnotationScroll: d.smoothAnnotationScroll,
		smoothPageTurn: d.smoothPageTurn,
		smoothBookmarkScroll: d.smoothBookmarkScroll,
		smoothSearchScroll: d.smoothSearchScroll,
		searchMarkers: d.searchMarkers,
		tooltipDelayMs: Number(d.tooltipDelayMs),
		openCommentAfterMarkup: d.openCommentAfterMarkup,
		rememberAnnotationStyle: d.rememberAnnotationStyle,
		rememberAnnotationSort: d.rememberAnnotationSort,
		annotationSort: { ...d.annotationSort },
		selectionHandles: d.selectionHandles,
		pdfaReadOnly: d.pdfaReadOnly,
		highlightLinks: d.highlightLinks,
		linkPreviews: d.linkPreviews,
		pageAdjustments: { ...d.pageAdjustments } satisfies PageAdjustments,
		colorPanelOnHover: d.colorPanelOnHover,
		presetColors: { ...d.presetColors },
		shortcuts: copyShortcuts(d.shortcuts)
	};
}

/** The draft for stored settings: the author left empty when it is the Windows user name. */
export function toDraft(s: Settings): Draft {
	return {
		...s,
		quickTools: [...s.quickTools],
		presetColors: { ...s.presetColors },
		pageAdjustments: { ...s.pageAdjustments },
		annotationSort: { ...s.annotationSort },
		shortcuts: copyShortcuts(s.shortcuts),
		author: s.author === s.defaultAuthor ? '' : s.author
	};
}
