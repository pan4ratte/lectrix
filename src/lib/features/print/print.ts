// Print (section 6.12, ADR 0028): what the Print dialog's choices mean, apart from the
// dialog. Rust lays the sheets out and prints them; this turns the choices into the pages
// to print, and says what the dialog shows of them.

import { resolvePageInput } from '#lib/features/viewer/pagebox.ts';
import type { PrintDuplex, PrintOrientation, PrintPreferences, PrintSizing } from '#lib/ipc/index.ts';

/** Which pages print. */
export type PagesChoice = 'all' | 'current' | 'range';

/** Every page chosen, or every other one of them. */
export type Subset = 'all' | 'odd' | 'even';

/** Smallest and largest custom scale, percent (mirrors `PRINT_SCALE` in `ipc.rs`). */
export const SCALE_RANGE = { min: 10, max: 1000 } as const;

/** Most copies a job asks for. */
export const MAX_COPIES = 999;

/** What the dialog starts with when nothing was printed before. */
export const DEFAULT_PREFERENCES: PrintPreferences = {
	printer: null,
	sizing: 'fit',
	scale: 100,
	orientation: 'auto',
	duplex: 'oneSided',
	annotations: true,
	grey: false,
	collate: true
};

export const SIZINGS: readonly { value: PrintSizing; label: string }[] = [
	{ value: 'fit', label: 'Fit to the paper' },
	{ value: 'actualSize', label: 'Actual size' },
	{ value: 'shrinkOversized', label: 'Shrink pages too large for the paper' },
	{ value: 'custom', label: 'Custom scale' }
];

export const ORIENTATIONS: readonly { value: PrintOrientation; label: string }[] = [
	{ value: 'auto', label: 'As the pages are' },
	{ value: 'portrait', label: 'Portrait' },
	{ value: 'landscape', label: 'Landscape' }
];

export const DUPLEXES: readonly { value: PrintDuplex; label: string }[] = [
	{ value: 'oneSided', label: 'One side' },
	{ value: 'longEdge', label: 'Both sides, flip on the long edge' },
	{ value: 'shortEdge', label: 'Both sides, flip on the short edge' }
];

export const SUBSETS: readonly { value: Subset; label: string }[] = [
	{ value: 'all', label: 'All of them' },
	{ value: 'odd', label: 'Odd pages only' },
	{ value: 'even', label: 'Even pages only' }
];

/**
 * The pages a range typed by the user names, in the order typed (0-based; a page may come
 * twice): "1-5, 8", by number or by label ("iv-x", "A-1", "A-1-A-5"), as the page box
 * takes them. An end left out runs to the first or last page ("5-", "-3"); a range that
 * runs backwards ("5-3") prints backwards. Null if any part names no page.
 */
export function parsePrintRange(text: string, labels: readonly string[] | null, count: number): number[] | null {
	const parts = text
		.split(/[,;]/)
		.map((p) => p.trim())
		.filter(Boolean);
	if (parts.length === 0 || count === 0) return null;
	const pages: number[] = [];
	for (const part of parts) {
		const range = resolvePart(part, labels, count);
		if (!range) return null;
		const [from, to] = range;
		const step = from <= to ? 1 : -1;
		for (let p = from; p !== to + step; p += step) pages.push(p);
	}
	return pages;
}

/** The first and last page of one part of a range. */
function resolvePart(part: string, labels: readonly string[] | null, count: number): [number, number] | null {
	// A page by itself, which may have a hyphen in its label ("A-1").
	const one = resolvePageInput(part, labels, count);
	if (one !== null) return [one, one];
	// Each hyphen or dash in turn, until both sides name pages.
	for (let i = 0; i < part.length; i++) {
		if (part[i] !== '-' && part[i] !== '–') continue;
		const left = part.slice(0, i).trim();
		const right = part.slice(i + 1).trim();
		const from = left === '' ? 0 : resolvePageInput(left, labels, count);
		const to = right === '' ? count - 1 : resolvePageInput(right, labels, count);
		if (from !== null && to !== null && !(left === '' && right === '')) return [from, to];
	}
	return null;
}

export interface PagesAsked {
	choice: PagesChoice;
	rangeText: string;
	/** The page shown, for `current`. */
	current: number;
	subset: Subset;
	reverse: boolean;
}

/** The pages to print, in order; null for a range that names no page, or none left. */
export function pagesToPrint(asked: PagesAsked, labels: readonly string[] | null, count: number): number[] | null {
	let pages: number[] | null;
	switch (asked.choice) {
		case 'all':
			pages = Array.from({ length: count }, (_, i) => i);
			break;
		case 'current':
			pages = asked.current >= 0 && asked.current < count ? [asked.current] : null;
			break;
		case 'range':
			pages = parsePrintRange(asked.rangeText, labels, count);
			break;
	}
	if (!pages) return null;
	// Odd and even by place among the pages chosen: the fronts and the backs of the sheets,
	// for printing on both sides by hand.
	if (asked.subset !== 'all') {
		const keep = asked.subset === 'odd' ? 0 : 1;
		pages = pages.filter((_, i) => i % 2 === keep);
	}
	if (asked.reverse) pages = [...pages].reverse();
	return pages.length > 0 ? pages : null;
}

/** A count of whole copies typed by the user, or null. */
export function parseCopies(text: string): number | null {
	const t = text.trim();
	if (!/^\d+$/.test(t)) return null;
	const n = Number(t);
	return n >= 1 && n <= MAX_COPIES ? n : null;
}

/** A custom scale typed by the user, percent, or null. */
export function parseScale(text: string): number | null {
	const t = text.trim().replace(/%$/, '').trim();
	if (!/^\d+$/.test(t)) return null;
	const n = Number(t);
	return n >= SCALE_RANGE.min && n <= SCALE_RANGE.max ? n : null;
}

/** "12 pages", "1 page". */
export function pagesText(n: number): string {
	return n === 1 ? '1 page' : `${n.toLocaleString()} pages`;
}

/** What the notification says once a job is with the printer. */
export function sentMessage(name: string, printer: string, sheets: number): string {
	return `Sent ${pagesText(sheets)} of ${name} to ${printer}.`;
}

/** The printer's name as the dropdown lists it, the system's default marked. */
export function printerLabel(name: string, isDefault: boolean): string {
	// Network printers are "\\server\queue": the queue's name is what people know.
	const short = name.startsWith('\\\\') ? `${name.split('\\').pop()} on ${name.split('\\')[2]}` : name;
	return isDefault ? `${short} (default)` : short;
}

/** The printer to start with: the one used last if it is still there, else the system's
 * default, else the first. */
export function initialPrinter(
	printers: readonly { name: string; default: boolean }[],
	remembered: string | null | undefined
): string | null {
	if (remembered && printers.some((p) => p.name === remembered)) return remembered;
	return (printers.find((p) => p.default) ?? printers[0])?.name ?? null;
}
