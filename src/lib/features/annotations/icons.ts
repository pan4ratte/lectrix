// The icon the annotation list shows for each annotation type (section 6.5), drawn in the
// annotation's own colour.

import {
	ChevronUp,
	Circle,
	EyeOff,
	Highlighter,
	MessageSquare,
	Minus,
	Paperclip,
	PenLine,
	Pentagon,
	Square,
	Stamp,
	StickyNote,
	Strikethrough,
	Type,
	Underline,
	Volume2,
	Waypoints,
	ZodiacAquarius
} from '@lucide/svelte';
import type { Component } from 'svelte';

type Icon = Component<{ size?: number; 'aria-hidden'?: boolean | 'true' }>;

const SUBTYPE_ICONS: Record<string, Icon> = {
	Highlight: Highlighter,
	Underline: Underline,
	StrikeOut: Strikethrough,
	Squiggly: ZodiacAquarius,
	Text: StickyNote,
	Ink: PenLine,
	FreeText: Type,
	Square: Square,
	Circle: Circle,
	Line: Minus,
	Polygon: Pentagon,
	PolyLine: Waypoints,
	Stamp: Stamp,
	Caret: ChevronUp,
	FileAttachment: Paperclip,
	Sound: Volume2,
	Redact: EyeOff
};

/** The icon for an annotation subtype; a speech bubble for types without one. */
export function typeIcon(subtype: string): Icon {
	return SUBTYPE_ICONS[subtype] ?? MessageSquare;
}
