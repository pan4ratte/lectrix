// Pages exported as images (section 6.14, ADR 0034): what the Export as images dialog's
// choices mean, apart from the dialog. Rust draws and writes the pages; this turns the
// choices into the pages and settings to export, checks them against each format's limits,
// and says what the dialog and the notification show.

import { fileSize } from '#lib/format.ts';
import { parsePrintRange } from '#lib/features/print/print.ts';
import { pageName } from '#lib/features/viewer/pagebox.ts';
import type {
	ExportDone,
	ExportFormat,
	ExportPreferences,
	ExportTarget,
	FileNaming,
	ImageColour,
	TiffCompression
} from '#lib/ipc/index.ts';
import { PLATFORM } from '#lib/platform.ts';

/** Which pages are exported. */
export type PagesChoice = 'all' | 'current' | 'selected' | 'range';

/** Lowest and highest resolution, dpi (mirrors `DPI_RANGE` in `export/mod.rs`). */
export const DPI_RANGE = { min: 10, max: 2400 } as const;

/** The most pixels a page has (mirrors `MAX_PIXELS`). */
export const MAX_PIXELS = 150_000_000;

/** The resolutions offered; any other is typed. */
export const DPI_PRESETS = [72, 96, 150, 300, 600] as const;

/** What the dialog starts with when nothing was exported before. */
export const DEFAULT_PREFERENCES: ExportPreferences = {
	format: 'png',
	dpi: 150,
	colour: 'colour',
	quality: 85,
	tiff: 'lzw',
	oneFile: false,
	naming: 'numbers',
	annotations: true
};

export const FORMATS: readonly { value: ExportFormat; label: string }[] = [
	{ value: 'png', label: 'PNG' },
	{ value: 'jpeg', label: 'JPEG' },
	{ value: 'tiff', label: 'TIFF' }
];

/** What each format is good for, said under its choice. */
export const FORMAT_NOTES: Record<ExportFormat, string> = {
	png: 'Every pixel kept. Sharp text and drawings; large for photos.',
	jpeg: 'Small for photos and scans; text and lines get soft edges.',
	tiff: 'Every pixel kept, for archives, printers and fax. Large.'
};

/** The extension each format's files get (mirrors `Format::extension`). */
export const EXTENSIONS: Record<ExportFormat, string> = {
	png: 'png',
	jpeg: 'jpg',
	tiff: 'tif'
};

/** The format's name in a sentence. */
export function formatName(format: ExportFormat): string {
	return FORMATS.find((f) => f.value === format)?.label ?? format;
}

export const TIFF_COMPRESSIONS: readonly { value: TiffCompression; label: string }[] = [
	{ value: 'lzw', label: 'LZW' },
	{ value: 'deflate', label: 'ZIP' }
];

/** Whether the format keeps black and white as one bit a pixel (PNG; TIFF, CCITT Group 4). */
export function takesBlackAndWhite(format: ExportFormat): boolean {
	return format === 'png' || format === 'tiff';
}

/** The colours offered for `format`. */
export function colours(format: ExportFormat): { value: ImageColour; label: string }[] {
	const all: { value: ImageColour; label: string }[] = [
		{ value: 'colour', label: 'Colour' },
		{ value: 'grey', label: 'Greyscale' },
		{ value: 'blackAndWhite', label: 'Black and white' }
	];
	return takesBlackAndWhite(format) ? all : all.slice(0, 2);
}

/** Whether `format` has a quality to choose. */
export function hasQuality(format: ExportFormat): boolean {
	return format === 'jpeg';
}

/** A resolution typed by the user, dpi, or null. */
export function parseDpi(text: string): number | null {
	const t = text.trim().replace(/\s*dpi$/i, '');
	if (!/^\d+$/.test(t)) return null;
	const n = Number(t);
	return n >= DPI_RANGE.min && n <= DPI_RANGE.max ? n : null;
}

export interface PagesAsked {
	choice: PagesChoice;
	rangeText: string;
	/** The page shown, for `current`. */
	current: number;
	/** The pages selected where the dialog was opened (Organize pages), for `selected`. */
	selected: readonly number[];
}

/** The pages to export, in order; null for a range that names no page, or none. */
export function pagesToExport(asked: PagesAsked, labels: readonly string[] | null, count: number): number[] | null {
	let pages: number[] | null;
	switch (asked.choice) {
		case 'all':
			pages = Array.from({ length: count }, (_, i) => i);
			break;
		case 'current':
			pages = asked.current >= 0 && asked.current < count ? [asked.current] : null;
			break;
		case 'selected':
			pages = asked.selected.filter((p) => p >= 0 && p < count);
			break;
		case 'range':
			pages = parsePrintRange(asked.rangeText, labels, count);
			break;
	}
	return pages && pages.length > 0 ? pages : null;
}

/** The pixels of a page `width` x `height` points at `dpi`, rounded as MuPDF rounds them
 * (mirrors `render::pixel_size`). */
export function pixelSize(width: number, height: number, dpi: number): { width: number; height: number } {
	const round = (v: number) => Math.max(1, Math.ceil(v - 0.001));
	const k = dpi / 72;
	return { width: round(width * k), height: round(height * k) };
}

/** The widest or tallest image `format` holds (mirrors `Format::max_side`). */
export function maxSide(format: ExportFormat): number {
	return format === 'jpeg' ? 65_535 : 1 << 20;
}

/** Whether a page `width` x `height` points fits `format` at `dpi`. */
export function fits(format: ExportFormat, dpi: number, width: number, height: number): boolean {
	const px = pixelSize(width, height, dpi);
	const side = maxSide(format);
	return px.width <= side && px.height <= side && px.width * px.height <= MAX_PIXELS;
}

export interface TooLarge {
	/** The first page too large (0-based), and its pixels. */
	page: number;
	width: number;
	height: number;
	/** The highest resolution at which every page fits, if one does. */
	maxDpi: number | null;
}

/** The first of `pages` too large for `format` at `dpi`, and the highest resolution all
 * fit at; null if every page fits. */
export function tooLarge(
	format: ExportFormat,
	dpi: number,
	pages: readonly number[],
	sizes: readonly { width: number; height: number }[]
): TooLarge | null {
	const first = pages.find((p) => sizes[p] && !fits(format, dpi, sizes[p].width, sizes[p].height));
	if (first === undefined) return null;
	const size = pixelSize(sizes[first]!.width, sizes[first]!.height, dpi);
	let best: number | null = null;
	for (let lo = DPI_RANGE.min, hi = dpi - 1; lo <= hi; ) {
		const mid = Math.floor((lo + hi) / 2);
		if (pages.every((p) => !sizes[p] || fits(format, mid, sizes[p].width, sizes[p].height))) {
			best = mid;
			lo = mid + 1;
		} else {
			hi = mid - 1;
		}
	}
	return { page: first, ...size, maxDpi: best };
}

/** What the dialog says of a page too large. */
export function tooLargeText(found: TooLarge, format: ExportFormat, labels: readonly string[] | null): string {
	const px = `${found.width.toLocaleString()} × ${found.height.toLocaleString()} pixels`;
	const at = found.maxDpi ? ` Choose ${found.maxDpi} dpi or less.` : ' Choose another format.';
	return `Page ${pageName(found.page, labels)} would be ${px}, too large for ${formatName(format)}.${at}`;
}

/** "12 pages", "1 page". */
export function pagesText(n: number): string {
	return n === 1 ? '1 page' : `${n.toLocaleString()} pages`;
}

/** The name a page's file is given, as an example: "report-007.png", "report-iv.png"
 * (mirrors `names::page_suffix`, without the repeats). */
export function exampleName(
	stem: string,
	page: number,
	count: number,
	naming: FileNaming,
	labels: readonly string[] | null,
	format: ExportFormat
): string {
	const label = naming === 'labels' ? safeLabel(labels?.[page] ?? '') : '';
	const suffix = label || String(page + 1).padStart(String(Math.max(1, count)).length, '0');
	return `${stem}-${suffix}.${EXTENSIONS[format]}`;
}

/** A label as part of a file name (mirrors `names::safe`). */
export function safeLabel(label: string): string {
	// eslint-disable-next-line no-control-regex
	return label.replace(/[<>:"/\\|?*\u0000-\u001f\u007f]/g, '_').trim().replace(/[. ]+$/, '');
}

/** The document's name without its extension: "report" for "report.pdf". */
export function stemOf(name: string): string {
	const dot = name.lastIndexOf('.');
	return dot > 0 ? name.slice(0, dot) : name;
}

/** What the dialog asks when files are already there. */
export function existingText(target: ExportTarget): string {
	const names = target.existing;
	const more = target.existingCount - names.length;
	const list =
		more > 0
			? `${names.join(', ')} and ${more.toLocaleString()} more`
			: names.length > 1
				? `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`
				: (names[0] ?? '');
	const count =
		target.existingCount === target.files
			? target.files === 2
				? 'Both files are'
				: `All ${target.files.toLocaleString()} files are`
			: target.existingCount === 1
				? `1 of these ${target.files.toLocaleString()} files is`
				: `${target.existingCount.toLocaleString()} of these ${target.files.toLocaleString()} files are`;
	return `${count} already in ${target.folder}: ${list}.`;
}

/** What the notification says once the files are written. */
export function doneMessage(name: string, done: ExportDone, format: ExportFormat): string {
	if (done.files === 0) return `Nothing was exported: every file was already in its folder.`;
	if (done.files === 1 && done.pages > 1) {
		return `Exported ${pagesText(done.pages)} of ${name} to ${done.first} in ${done.folder}.`;
	}
	if (done.pages === 1) return `Exported a page of ${name} to ${done.first} in ${done.folder}.`;
	return `Exported ${pagesText(done.pages)} of ${name} as ${formatName(format)} images to ${done.folder}.`;
}

/** The size of a page's file, as the preview says it; "about" when estimated. */
export function previewSizeText(bytes: number, estimated: boolean): string {
	return `${estimated ? 'about ' : ''}${fileSize(bytes)}`;
}

/** What the system's file manager calls showing a file in its folder. */
export const SHOW_IN_FOLDER =
	PLATFORM === 'macos' ? 'Show in Finder' : PLATFORM === 'linux' ? 'Show in folder' : 'Open file location';
