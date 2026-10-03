// Annotation tools and their styles (section 6.5), and the words the UI uses for
// annotation types and problems.

import type { Annotation, AnnotationKind, AnnotationProblem } from '#lib/ipc/index.ts';

export type Tool = 'select' | 'highlight' | 'underline' | 'strikeOut' | 'squiggly' | 'note' | 'ink' | 'freeText';
/** Tools that create something. */
export type DrawTool = Exclude<Tool, 'select'>;

export const MARKUP_TOOLS: readonly DrawTool[] = ['highlight', 'underline', 'strikeOut', 'squiggly'];

export function isMarkupTool(tool: Tool): tool is 'highlight' | 'underline' | 'strikeOut' | 'squiggly' {
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

/** Six preset colours (section 6.5), chosen to read well as highlights and as lines. */
export const PRESET_COLORS: readonly { value: string; name: string }[] = [
	{ value: '#ffd400', name: 'Yellow' },
	{ value: '#5fd35f', name: 'Green' },
	{ value: '#33a7ff', name: 'Blue' },
	{ value: '#ff6fae', name: 'Pink' },
	{ value: '#e53935', name: 'Red' },
	{ value: '#202020', name: 'Black' }
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

export const DEFAULT_STYLES: Readonly<Record<DrawTool, ToolStyle>> = {
	highlight: { color: '#ffd400', opacity: 1, width: 2, fontSize: 12 },
	underline: { color: '#5fd35f', opacity: 1, width: 2, fontSize: 12 },
	strikeOut: { color: '#e53935', opacity: 1, width: 2, fontSize: 12 },
	squiggly: { color: '#33a7ff', opacity: 1, width: 2, fontSize: 12 },
	note: { color: '#ffd400', opacity: 1, width: 2, fontSize: 12 },
	ink: { color: '#e53935', opacity: 1, width: 2, fontSize: 12 },
	freeText: { color: '#202020', opacity: 1, width: 2, fontSize: 12 }
};

export const PEN_WIDTHS: readonly number[] = [0.5, 1, 2, 3, 5, 8];
export const FONT_SIZES: readonly number[] = [8, 9, 10, 11, 12, 14, 16, 18, 24, 36];

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
	return {
		restyle: editable && RESTYLABLE.has(a.subtype),
		text: editable && MARKUP.has(a.subtype) && a.replyTo === null,
		move: editable && (a.kind === 'note' || a.kind === 'ink' || a.kind === 'freeText'),
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
	malformedQuads: 'marks text with corner data that can’t be read (Folio can’t fix this)'
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

export function needsRepair(a: Annotation): boolean {
	return a.problems.length > 0;
}

/** A date for the inspector and the list, in the user's locale. */
export function formatDate(ms: number | null): string {
	if (ms === null) return '—';
	return new Date(ms).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
}
