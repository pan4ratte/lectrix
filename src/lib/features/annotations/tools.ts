// Annotation tools and their styles (section 6.5), and the words the UI uses for
// annotation types and problems.

import type { Annotation, AnnotationKind, AnnotationProblem, QuickTool } from '#lib/ipc/index.ts';

export type Tool = 'select' | 'highlight' | 'underline' | 'strikeOut' | 'squiggly' | 'note' | 'ink' | 'freeText';
/** Tools that create something. */
export type DrawTool = Exclude<Tool, 'select'>;
export type MarkupKind = 'highlight' | 'underline' | 'strikeOut' | 'squiggly';

export const MARKUP_TOOLS: readonly MarkupKind[] = ['highlight', 'underline', 'strikeOut', 'squiggly'];

export function isMarkupTool(tool: Tool): tool is MarkupKind {
	return (MARKUP_TOOLS as readonly Tool[]).includes(tool);
}

export interface ToolInfo {
	id: Tool;
	label: string;
	/** Single key that picks the tool (section 8). */
	key?: string;
}

export const TOOLS: readonly ToolInfo[] = [
	{ id: 'select', label: 'Select', key: 'Esc' },
	{ id: 'highlight', label: 'Highlight', key: 'H' },
	{ id: 'underline', label: 'Underline', key: 'U' },
	{ id: 'strikeOut', label: 'Strikeout' },
	{ id: 'squiggly', label: 'Squiggly underline' },
	{ id: 'note', label: 'Note', key: 'N' },
	{ id: 'ink', label: 'Pen', key: 'P' },
	{ id: 'freeText', label: 'Text box', key: 'T' }
];

/** The buttons the bar over selected text can have (Settings), in the order it shows them. */
export const QUICK_TOOLS: readonly { id: QuickTool; label: string }[] = [
	{ id: 'highlight', label: 'Highlight' },
	{ id: 'underline', label: 'Underline' },
	{ id: 'strikeOut', label: 'Strikeout' },
	{ id: 'squiggly', label: 'Squiggly underline' },
	{ id: 'highlightNote', label: 'Highlight with note' },
	{ id: 'copy', label: 'Copy' },
	{ id: 'bookmark', label: 'Add bookmark' }
];

/** The quick tools until Settings are loaded (mirrors QuickTool::DEFAULT in Rust). */
export const DEFAULT_QUICK_TOOLS: readonly QuickTool[] = ['highlight', 'underline', 'strikeOut', 'highlightNote', 'copy'];

/** Text-markup subtypes, which can be turned into one another. */
export const MARKUP_SUBTYPES: readonly { subtype: string; kind: MarkupKind }[] = [
	{ subtype: 'Highlight', kind: 'highlight' },
	{ subtype: 'Underline', kind: 'underline' },
	{ subtype: 'StrikeOut', kind: 'strikeOut' },
	{ subtype: 'Squiggly', kind: 'squiggly' }
];

/** Acrobat's yellow and red, as it writes them (`/C [1 0.819611 0]`, `/C [0.898041
 * 0.133331 0.215683]`), so its annotations show these swatches as their colour. */
const ACROBAT_YELLOW = '#ffd100';
const ACROBAT_RED = '#e52237';

/** Columns of the preset grid. */
export const PRESET_COLUMNS = 6;

/**
 * Acrobat's 18 preset colours (section 6.5), in its grid of three rows: bright colours,
 * their light tints, and white to black. Read from Acrobat DC's own table (`Acrobat.dll`,
 * beside `AVInPlaceColorPickerView`), so the swatches match the colours its annotations have.
 */
export const PRESET_COLORS: readonly { value: string; name: string }[] = [
	{ value: '#0000ff', name: 'Blue' },
	{ value: '#6ad928', name: 'Green' },
	{ value: ACROBAT_YELLOW, name: 'Yellow' },
	{ value: '#ff7002', name: 'Orange' },
	{ value: ACROBAT_RED, name: 'Red' },
	{ value: '#a33086', name: 'Purple' },
	{ value: '#38e5ff', name: 'Light blue' },
	{ value: '#c5fb72', name: 'Light green' },
	{ value: '#fcf485', name: 'Light yellow' },
	{ value: '#ffc09e', name: 'Light orange' },
	{ value: '#ff809d', name: 'Pink' },
	{ value: '#fb88ff', name: 'Light purple' },
	{ value: '#ffffff', name: 'White' },
	{ value: '#cccccc', name: 'Light grey' },
	{ value: '#aaaaaa', name: 'Grey' },
	{ value: '#777777', name: 'Dark grey' },
	{ value: '#444444', name: 'Charcoal' },
	{ value: '#000000', name: 'Black' }
];

export interface ToolStyle {
	/** #rrggbb */
	color: string;
	/** 0.1 to 1 */
	opacity: number;
	/** Pen stroke width, points. */
	width: number;
	/** Text box font size, points. */
	fontSize: number;
}

/**
 * Each tool's style until the user changes it: the colours and opacity Acrobat gives new
 * annotations (highlights yellow at 40%; notes yellow; underline, strikeout, pen and
 * text-box text red). Acrobat has no squiggly tool; it gets the red of the other lines.
 */
export const DEFAULT_STYLES: Readonly<Record<DrawTool, ToolStyle>> = {
	highlight: { color: ACROBAT_YELLOW, opacity: 0.4, width: 2, fontSize: 12 },
	underline: { color: ACROBAT_RED, opacity: 1, width: 2, fontSize: 12 },
	strikeOut: { color: ACROBAT_RED, opacity: 1, width: 2, fontSize: 12 },
	squiggly: { color: ACROBAT_RED, opacity: 1, width: 2, fontSize: 12 },
	note: { color: ACROBAT_YELLOW, opacity: 1, width: 2, fontSize: 12 },
	ink: { color: ACROBAT_RED, opacity: 1, width: 2, fontSize: 12 },
	freeText: { color: ACROBAT_RED, opacity: 1, width: 2, fontSize: 12 }
};

/** The parts of `styles` that differ from the defaults: what is remembered of them. */
export function changedStyles(styles: Readonly<Record<DrawTool, ToolStyle>>): Partial<Record<DrawTool, Partial<ToolStyle>>> {
	const changed: Partial<Record<DrawTool, Partial<ToolStyle>>> = {};
	for (const tool of Object.keys(DEFAULT_STYLES) as DrawTool[]) {
		const diff = Object.fromEntries(
			Object.entries(styles[tool]).filter(([key, v]) => v !== DEFAULT_STYLES[tool][key as keyof ToolStyle])
		) as Partial<ToolStyle>;
		if (Object.keys(diff).length) changed[tool] = diff;
	}
	return changed;
}

export const PEN_WIDTHS: readonly number[] = [0.5, 1, 2, 3, 5, 8];
export const FONT_SIZES: readonly number[] = [8, 9, 10, 11, 12, 14, 16, 18, 24, 36];

/** Sizes in points as dropdown options, smallest first, each once. */
export function ptOptions(sizes: readonly number[]): { value: number; label: string }[] {
	return [...new Set(sizes)].sort((x, y) => x - y).map((value) => ({ value, label: `${value} pt` }));
}

/** The tool that makes annotations of `kind`. */
export function toolFor(kind: AnnotationKind): DrawTool {
	return kind;
}

const SUBTYPE_NAMES: Record<string, string> = {
	Highlight: 'Highlight',
	Underline: 'Underline',
	StrikeOut: 'Strikeout',
	Squiggly: 'Squiggly underline',
	Text: 'Note',
	Ink: 'Drawing',
	FreeText: 'Text box',
	Square: 'Rectangle',
	Circle: 'Ellipse',
	Line: 'Line',
	Polygon: 'Polygon',
	PolyLine: 'Polyline',
	Stamp: 'Stamp',
	Caret: 'Insertion mark',
	FileAttachment: 'Attached file',
	Sound: 'Sound',
	Redact: 'Redaction mark'
};

/** What the UI calls an annotation's type. */
export function typeName(subtype: string): string {
	return SUBTYPE_NAMES[subtype] ?? subtype;
}

/** Subtypes whose colour and opacity can be changed (mirrors pdf-core's RESTYLABLE). */
const RESTYLABLE = new Set([
	'Text',
	'FreeText',
	'Line',
	'Square',
	'Circle',
	'Polygon',
	'PolyLine',
	'Highlight',
	'Underline',
	'Squiggly',
	'StrikeOut',
	'Caret',
	'Ink'
]);

/** Subtypes with a note and an author (markup annotations; mirrors pdf-core's MARKUP). */
const MARKUP = new Set([...RESTYLABLE, 'Stamp', 'FileAttachment', 'Sound', 'Redact']);

export interface Capabilities {
	restyle: boolean;
	/** Note text (a text box: its text) and author. */
	text: boolean;
	move: boolean;
	resize: boolean;
	delete: boolean;
}

/** What may be changed on `a` (nothing in a document that doesn't allow annotating). */
export function capabilities(a: Annotation, canAnnotate: boolean): Capabilities {
	const editable = canAnnotate && a.id !== 0;
	// A reply (a note answering another annotation) is never drawn: only its text and author
	// change (pdf-core's `reply::edit_reply`, ADR 0012).
	const reply = a.replyTo !== null && a.subtype === 'Text';
	return {
		restyle: editable && !reply && RESTYLABLE.has(a.subtype),
		text: editable && MARKUP.has(a.subtype),
		move: editable && !reply && (a.kind === 'note' || a.kind === 'ink' || a.kind === 'freeText'),
		resize: editable && (a.kind === 'ink' || a.kind === 'freeText'),
		delete: editable
	};
}

/** Plain words for problems repair fixes (section 5.3). */
export const PROBLEM_TEXT: Readonly<Record<AnnotationProblem, string>> = {
	missingAppearance: 'has no appearance stream, so some apps don’t show it',
	quadOrder: 'marks text with corners in an order some apps misread',
	rectTooSmall: 'has bounds smaller than what it draws, so some apps cut it off',
	missingName: 'has no unique name',
	missingFlags: 'isn’t set to print',
	missingModified: 'has no modification date',
	missingPage: 'doesn’t say which page it is on',
	malformedQuads: 'marks text with corner data that can’t be read (Lectrix can’t fix this)'
};

/** Short form for the summary list. */
export const PROBLEM_SUMMARY: Readonly<Record<AnnotationProblem, string>> = {
	missingAppearance: 'No appearance stream',
	quadOrder: 'Text marked in the wrong corner order',
	rectTooSmall: 'Bounds too small',
	missingName: 'No unique name',
	missingFlags: 'Not set to print',
	missingModified: 'No modification date',
	missingPage: 'No page reference',
	malformedQuads: 'Unreadable text marking (can’t be fixed)'
};

/**
 * Whether selecting `a` shows its comment panel rather than its bar: it has a comment. A
 * text box's text is on the page, so it gets its bar.
 */
export function showsComment(a: Annotation): boolean {
	return a.subtype !== 'FreeText' && a.contents.trim() !== '';
}

export function needsRepair(a: Annotation): boolean {
	return a.problems.length > 0;
}

/** A date for the inspector and the list, in the user's locale. */
export function formatDate(ms: number | null, locale?: string): string {
	if (ms === null) return '—';
	return new Date(ms).toLocaleString(locale, { dateStyle: 'medium', timeStyle: 'short' });
}

/**
 * `text` with its line breaks as LF. Acrobat separates lines with a lone CR, which HTML
 * shows as a space; a text field would also turn it into LF, so leaving a field would count
 * as an edit and rewrite a comment nobody changed. Only edited comments are written back.
 */
export function lineBreaks(text: string): string {
	return text.replace(/\r\n?|\u2028|\u2029/g, '\n');
}

/**
 * A date as short as the annotation list needs, as Acrobat shows it: the time for today
 * ("21:42"), the day and month this year ("5 Oct"), and the year too before that.
 */
export function shortDate(ms: number, now: number = Date.now(), locale?: string): string {
	const d = new Date(ms);
	const today = new Date(now);
	if (d.toDateString() === today.toDateString()) {
		return d.toLocaleTimeString(locale, { timeStyle: 'short' });
	}
	if (d.getFullYear() === today.getFullYear()) {
		return d.toLocaleDateString(locale, { day: 'numeric', month: 'short' });
	}
	return d.toLocaleDateString(locale, { day: 'numeric', month: 'short', year: 'numeric' });
}
