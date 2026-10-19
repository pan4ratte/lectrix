// What Compress shows and says (section 6.11, ADR 0027): the choices of its dialog, the
// simple form of its settings, sizes, progress, the result and the notifications.

import type {
	CompressAnalysis,
	CompressOutcome,
	CompressPreset,
	CompressPresets,
	CompressProgress,
	CompressRemovals,
	CompressSettings,
	DownsampleSettings,
	ImageColorMode,
	ImageCompression,
	ResampleMethod
} from '#lib/ipc/index.ts';
import { fileSize } from '#lib/format.ts';

export const PRESETS: readonly { value: CompressPreset; label: string; detail: string }[] = [
	{
		value: 'lossless',
		label: 'Lossless',
		detail: 'Nothing visible changes: images packed tighter without losing a pixel, fonts subset, structure compacted.'
	},
	{ value: 'highQuality', label: 'High quality', detail: 'For print: images at 300 dpi, JPEG quality 85.' },
	{
		value: 'balanced',
		label: 'Balanced',
		detail: 'For reading on screen and sharing: images at 150 dpi, JPEG quality 75.'
	},
	{ value: 'smallest', label: 'Smallest', detail: 'For email: images at 96 dpi, JPEG quality 50.' },
	{ value: 'custom', label: 'Custom', detail: 'Settings of your own.' }
];

export const COMPRESSIONS: readonly { value: ImageCompression; label: string }[] = [
	{ value: 'jpeg', label: 'JPEG' },
	{ value: 'jpeg2000', label: 'JPEG 2000' },
	{ value: 'keep', label: 'Keep as it is' }
];

export const METHODS: readonly { value: ResampleMethod; label: string }[] = [
	{ value: 'bicubic', label: 'Bicubic (sharp)' },
	{ value: 'average', label: 'Average (smooth)' }
];

export const COLOR_MODES: readonly { value: ImageColorMode; label: string }[] = [
	{ value: 'keep', label: 'Keep colours' },
	{ value: 'gray', label: 'Greyscale' },
	{ value: 'blackAndWhite', label: 'Black and white' }
];

/** The resolutions downsampling takes, as `pdf_core::compress::validate` checks them. */
export const MIN_DPI = 9;
export const MAX_DPI = 2400;

/** The settings of `preset` (Custom: Balanced's). */
export function presetSettings(presets: CompressPresets, preset: CompressPreset): CompressSettings {
	switch (preset) {
		case 'lossless':
			return presets.lossless;
		case 'highQuality':
			return presets.highQuality;
		case 'smallest':
			return presets.smallest;
		default:
			return presets.balanced;
	}
}

/** What the settings change, without their preset's name or how they are shown. */
function essence(s: CompressSettings): string {
	return JSON.stringify([s.color, s.gray, s.mono, s.colorMode, s.subsetFonts, s.objectStreams, s.remove]);
}

/** The preset whose settings these are, or Custom. */
export function matchingPreset(settings: CompressSettings, presets: CompressPresets): CompressPreset {
	const found = PRESETS.find(
		(p) => p.value !== 'custom' && essence(presetSettings(presets, p.value)) === essence(settings)
	);
	return found ? found.value : 'custom';
}

/** Black-and-white images in the simple form: twice the resolution set, at least 300 dpi,
 * above one and a half times that. */
export function simpleMono(color: DownsampleSettings): DownsampleSettings {
	const toDpi = Math.max(300, color.toDpi * 2);
	return { enabled: color.enabled, toDpi, aboveDpi: Math.round(toDpi * 1.5), method: color.method };
}

/** The settings as the simple form makes them: greyscale images as colour ones, black and
 * white as `simpleMono` says. */
export function simplified(s: CompressSettings): CompressSettings {
	return {
		...s,
		gray: { ...s.color, downsample: { ...s.color.downsample } },
		mono: simpleMono(s.color.downsample)
	};
}

/** `s` with its preset named anew after a change. */
export function renamed(s: CompressSettings, presets: CompressPresets): CompressSettings {
	return { ...s, preset: matchingPreset(s, presets) };
}

/** What is wrong with a downsampling's numbers, said under them; null if nothing. */
export function downsampleProblem(d: DownsampleSettings): string | null {
	if (!d.enabled) return null;
	const ok = (v: number) => Number.isInteger(v) && v >= MIN_DPI && v <= MAX_DPI;
	if (!ok(d.toDpi) || !ok(d.aboveDpi)) return `Enter resolutions from ${MIN_DPI} to ${MAX_DPI} dpi.`;
	if (d.aboveDpi < d.toDpi) return 'The first resolution can’t be lower than the second.';
	return null;
}

/** The settings can be sent: every downsampling in range, qualities from 1 to 100. */
export function settingsValid(s: CompressSettings): boolean {
	const quality = (q: number) => Number.isInteger(q) && q >= 1 && q <= 100;
	return (
		[s.color.downsample, s.gray.downsample, s.mono].every((d) => downsampleProblem(d) === null) &&
		quality(s.color.quality) &&
		quality(s.gray.quality)
	);
}

/** A size as the app writes sizes everywhere ("820 KB", "6.1 MB", "48 MB"). */
export function formatSize(bytes: number, locale?: string): string {
	return fileSize(bytes, locale);
}

/** How much smaller, in whole percent ("87% smaller"; "less than 1% smaller"). */
export function savedPercent(before: number, after: number): string {
	if (before <= 0 || after >= before) return '0%';
	const percent = Math.round((1 - after / before) * 100);
	return percent < 1 ? 'less than 1%' : `${percent}%`;
}

export interface SizePart {
	key: 'images' | 'fonts' | 'content' | 'attachments' | 'thumbnails' | 'privateData' | 'other';
	label: string;
	bytes: number;
}

/** What the file's size is made of, largest first, without the empty parts. */
export function sizeParts(a: CompressAnalysis): SizePart[] {
	const parts: SizePart[] = [
		{ key: 'images', label: 'Images', bytes: a.images.bytes },
		{ key: 'fonts', label: 'Fonts', bytes: a.fonts.bytes },
		{ key: 'content', label: 'Page content', bytes: a.content.bytes },
		{ key: 'attachments', label: 'Attachments', bytes: a.attachments.bytes },
		{ key: 'thumbnails', label: 'Thumbnails', bytes: a.thumbnails.bytes },
		{ key: 'privateData', label: 'Other apps’ data', bytes: a.privateData.bytes }
	];
	const shown = parts.filter((p) => p.bytes > 0).sort((x, y) => y.bytes - x.bytes);
	const other = a.other + a.metadata.bytes;
	if (other > 0) shown.push({ key: 'other', label: 'Other', bytes: other });
	return shown;
}

function count(n: number, one: string, many: string): string {
	return `${n.toLocaleString()} ${n === 1 ? one : many}`;
}

export interface RemovalRow {
	key: keyof CompressRemovals;
	label: string;
	detail: string;
}

/** The removals the dialog offers: only what the file has. */
export function removalRows(a: CompressAnalysis): RemovalRow[] {
	const rows: RemovalRow[] = [];
	const f = a.found;
	if (f.thumbnails > 0)
		rows.push({
			key: 'thumbnails',
			label: 'Page thumbnails',
			detail: `${count(f.thumbnails, 'page', 'pages')}, ${formatSize(a.thumbnails.bytes)}. Readers draw their own.`
		});
	if (f.privateData > 0)
		rows.push({
			key: 'privateData',
			label: 'Other apps’ editing data',
			detail:
				'Illustrator and Photoshop data the PDF doesn’t show' +
				(a.privateData.bytes > 0 ? `, ${formatSize(a.privateData.bytes)}.` : '.')
		});
	if (f.attachments > 0)
		rows.push({
			key: 'attachments',
			label: 'Attached files',
			detail: `${count(f.attachments, 'file', 'files')}, ${formatSize(a.attachments.bytes)}.`
		});
	if (f.javascript > 0)
		rows.push({
			key: 'javascript',
			label: 'JavaScript',
			detail: `${count(f.javascript, 'script', 'scripts')}. Forms that calculate or check their fields stop doing so.`
		});
	if (f.tags)
		rows.push({
			key: 'tags',
			label: 'Tags for accessibility',
			detail: 'Screen readers and reflow use them. Remove them only if no one needs to read the file aloud.'
		});
	return rows;
}

/** What compressing is doing, under its progress bar. */
export function stageText(p: CompressProgress): string {
	switch (p.stage) {
		case 'reading':
			return `Reading pages… ${p.done.toLocaleString()} of ${p.total.toLocaleString()}`;
		case 'images':
			return `Compressing images… ${p.done.toLocaleString()} of ${p.total.toLocaleString()}`;
		case 'fonts':
			return 'Subsetting fonts…';
		case 'writing':
			return 'Writing…';
	}
}

/** How far the progress bar is, out of 100; null for a stage without a count. */
export function stageValue(p: CompressProgress): number | null {
	if ((p.stage === 'reading' || p.stage === 'images') && p.total > 0) return (p.done / p.total) * 100;
	return null;
}

/** "48.2 MB → 6.1 MB, 87% smaller". */
export function headline(o: CompressOutcome): string {
	return `${formatSize(o.before)} → ${formatSize(o.after)}, ${savedPercent(o.before, o.after)} smaller`;
}

/** What compressing did, a sentence each. */
export function details(o: CompressOutcome): string[] {
	const lines: string[] = [];
	if (o.images > 0) {
		let line = `${count(o.images, 'image', 'images')}: ${o.imagesReplaced.toLocaleString()} recompressed, ${o.imagesDownsampled.toLocaleString()} downsampled.`;
		if (o.imagesSkipped > 0) line += ` ${o.imagesSkipped.toLocaleString()} left as they were.`;
		lines.push(line);
	}
	if (o.fontsSubset > 0) lines.push(`${count(o.fontsSubset, 'font', 'fonts')} subset.`);
	const r = o.removed;
	const removed = [
		r.thumbnails > 0 ? count(r.thumbnails, 'thumbnail', 'thumbnails') : null,
		r.privateData > 0 ? 'other apps’ editing data' : null,
		r.attachments > 0 ? count(r.attachments, 'attached file', 'attached files') : null,
		r.javascript > 0 ? count(r.javascript, 'script', 'scripts') : null,
		r.tags ? 'tags' : null
	].filter((x): x is string => x !== null);
	if (removed.length > 0) lines.push(`Removed: ${removed.join(', ')}.`);
	return lines;
}

/** Said when the result is not smaller than the file. */
export function notSmallerText(name: string, o: CompressOutcome): string {
	return `${name} is already compact. Compressed with these settings it would be ${formatSize(o.after)}, no smaller than it is.`;
}

/** The warnings the settings show, as they apply. */
export const SIGNED_WARNING =
	'This file is signed. Its signatures will not be valid in the compressed file, because the whole file is rewritten.';

export function pdfaWarning(level: string): string {
	return `This file claims compliance with ${level}. The compressed file will not claim it.`;
}

/** Asked before a document with unsaved changes is compressed: Compress works on its file. */
export function saveFirstQuestion(name: string) {
	return {
		title: 'Save your changes first?',
		message: `${name} has unsaved changes. Compressing works on the file as it is saved, so save them first.`,
		buttons: [
			{ id: 'save', label: 'Save', primary: true },
			{ id: 'cancel', label: 'Cancel' }
		],
		cancel: 'cancel'
	};
}

/** Said once the result is saved (and opened in a tab): `replaced` when over the file. */
export function compressedMessage(name: string, replaced: boolean, before: number, after: number) {
	return replaced
		? { kind: 'info' as const, message: `${name} is now ${formatSize(after)} (it was ${formatSize(before)}).` }
		: {
				kind: 'info' as const,
				message: `Saved ${name}: ${formatSize(after)}, ${savedPercent(before, after)} smaller.`,
				suggestion: 'The original file is unchanged.'
			};
}
