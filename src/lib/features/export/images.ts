// The images in a document exported (section 6.14, ADR 0035): what the Images in the
// document dialog's choices mean, apart from the dialog. Rust finds and writes the images;
// this says what the dialog and the notification show.

import { fileSize } from '#lib/format.ts';
import type { EmbeddedKind, FileNaming, FoundImage, ImagesDone, ImagesFound, ImagesPreferences } from '#lib/ipc/index.ts';

import { safeLabel } from './export.ts';

/** What the dialog starts with when nothing was exported before. */
export const DEFAULT_IMAGES_PREFERENCES: ImagesPreferences = {
	skipSmall: true,
	minSide: 32,
	naming: 'numbers'
};

/** The sizes offered for images to leave out, pixels. */
export const MIN_SIDES = [16, 32, 64, 128, 256] as const;

/** Each kind's name in a sentence. */
export const KIND_NAMES: Record<EmbeddedKind, string> = {
	jpeg: 'JPEG',
	jpeg2000: 'JPEG 2000',
	png: 'PNG'
};

/** "12 images", "1 image". */
export function imagesText(n: number): string {
	return n === 1 ? '1 image' : `${n.toLocaleString()} images`;
}

/** A list in a sentence: "a", "a and b", "a, b and c". */
function listed(items: readonly string[]): string {
	if (items.length <= 1) return items[0] ?? '';
	return `${items.slice(0, -1).join(', ')} and ${items[items.length - 1]}`;
}

/** What the dialog says of the images exported: "37 images: 30 JPEG and 7 PNG, about
 * 24 MB." (exactly, with none of them PNG). */
export function summaryText(found: ImagesFound): string {
	const images = found.images;
	if (images.length === 0) return '';
	const counts = new Map<EmbeddedKind, number>();
	for (const image of images) counts.set(image.kind, (counts.get(image.kind) ?? 0) + 1);
	const order: EmbeddedKind[] = ['jpeg', 'jpeg2000', 'png'];
	const kinds = order.filter((k) => counts.has(k)).map((k) => `${counts.get(k)!.toLocaleString()} ${KIND_NAMES[k]}`);
	const bytes = images.reduce((sum, image) => sum + image.bytes, 0);
	const estimated = counts.has('png');
	const size = `${estimated ? 'about ' : ''}${fileSize(bytes)}`;
	return `${imagesText(images.length)}: ${listed(kinds)}, ${size}.`;
}

/** What the dialog says of the images left out, or ''. */
export function leftOutText(found: ImagesFound, minSide: number | null): string {
	const parts: string[] = [];
	if (found.repeats > 0) {
		parts.push(
			found.repeats === 1
				? 'an image drawn again, or alike, exported once'
				: `${found.repeats.toLocaleString()} images drawn again, or alike, exported once`
		);
	}
	if (found.small > 0 && minSide !== null) {
		parts.push(
			found.small === 1
				? `1 image smaller than ${minSide} pixels`
				: `${found.small.toLocaleString()} images smaller than ${minSide} pixels`
		);
	}
	if (parts.length === 0) return '';
	const text = `Left out: ${listed(parts)}.`;
	return text;
}

/** What the dialog says when there is nothing to export. */
export function noneText(found: ImagesFound, minSide: number | null, pages: number): string {
	const where = pages === 1 ? 'this page' : 'these pages';
	if (found.small > 0 && minSide !== null) {
		return `Every image on ${where} is smaller than ${minSide} pixels.`;
	}
	return `There are no images on ${where}.`;
}

/** The name an image's file is given, as an example: "report-007-1.jpg", "report-iv-2.png"
 * (mirrors `names::image_file_names`, without the repeats). */
export function exampleImageName(
	stem: string,
	image: FoundImage,
	count: number,
	most: number,
	naming: FileNaming,
	labels: readonly string[] | null
): string {
	const label = naming === 'labels' ? safeLabel(labels?.[image.page] ?? '') : '';
	const page = label || String(image.page + 1).padStart(String(Math.max(1, count)).length, '0');
	const index = String(image.index).padStart(String(Math.max(1, most)).length, '0');
	return `${stem}-${page}-${index}.${extensionOf(image.kind)}`;
}

/** The extension an image's file gets (a JPEG 2000 codestream's, `.j2k`, is Rust's to say). */
export function extensionOf(kind: EmbeddedKind): string {
	return kind === 'jpeg' ? 'jpg' : kind === 'jpeg2000' ? 'jp2' : 'png';
}

/** What a thumbnail's tip says of an image: "Page 7 · 1,200 × 800 pixels · JPEG, 120 KB". */
export function imageTip(image: FoundImage, pageName: string): string {
	const size = `${image.width.toLocaleString()} × ${image.height.toLocaleString()} pixels`;
	return `Page ${pageName} · ${size} · ${KIND_NAMES[image.kind]}, ${imageSizeText(image)}`;
}

/** What the notification says once the files are written. */
export function imagesDoneMessage(name: string, done: ImagesDone): string {
	if (done.files === 0 && done.unreadable === 0) return 'Nothing was exported: every file was already in its folder.';
	const unreadable =
		done.unreadable === 0
			? ''
			: done.unreadable === 1
				? ' 1 image couldn’t be read and was left out.'
				: ` ${done.unreadable.toLocaleString()} images couldn’t be read and were left out.`;
	if (done.files === 0) return 'Nothing was exported: the images couldn’t be read.';
	if (done.files === 1) return `Exported an image of ${name} to ${done.first} in ${done.folder}.${unreadable}`;
	return `Exported ${imagesText(done.files)} from ${name} to ${done.folder}.${unreadable}`;
}

/** The size of an image's file, as the thumbnail's tip says it. */
export function imageSizeText(image: FoundImage): string {
	return `${image.kind === 'png' ? 'about ' : ''}${fileSize(image.bytes)}`;
}
