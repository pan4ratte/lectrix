import { describe, expect, it } from 'vitest';

import {
	DPI_RANGE,
	colours,
	copiedMessage,
	doneMessage,
	exampleName,
	existingText,
	fits,
	hasQuality,
	pagesToExport,
	parseDpi,
	pixelSize,
	safeLabel,
	stemOf,
	tooLarge,
	tooLargeText
} from './export.ts';

const labels = ['i', 'ii', 'iii', '1', '2', '3', 'A:1', 'A-2'];
const letter = { width: 612, height: 792 };

describe('the pages exported', () => {
	const asked = { choice: 'all' as const, rangeText: '', current: 2, selected: [] as number[] };

	it('all, the current, the selected, or a range', () => {
		expect(pagesToExport(asked, null, 4)).toEqual([0, 1, 2, 3]);
		expect(pagesToExport({ ...asked, choice: 'current' }, null, 4)).toEqual([2]);
		expect(pagesToExport({ ...asked, choice: 'selected', selected: [3, 1] }, null, 4)).toEqual([3, 1]);
		expect(pagesToExport({ ...asked, choice: 'range', rangeText: 'ii-1, A-2' }, labels, 8)).toEqual([1, 2, 3, 7]);
	});

	it('none, for a range that names no page', () => {
		expect(pagesToExport({ ...asked, choice: 'range', rangeText: '12' }, null, 4)).toBeNull();
		expect(pagesToExport({ ...asked, choice: 'selected', selected: [] }, null, 4)).toBeNull();
	});
});

describe('resolution and size', () => {
	it('takes a resolution from 10 to 2,400 dpi', () => {
		expect(parseDpi('300')).toBe(300);
		expect(parseDpi(' 450 dpi ')).toBe(450);
		expect(parseDpi('9')).toBeNull();
		expect(parseDpi(String(DPI_RANGE.max + 1))).toBeNull();
		expect(parseDpi('1.5')).toBeNull();
	});

	it('counts pixels as MuPDF rounds them', () => {
		expect(pixelSize(612, 792, 150)).toEqual({ width: 1275, height: 1650 });
		expect(pixelSize(595, 842, 600)).toEqual({ width: 4959, height: 7017 });
	});

	it('knows what each format holds', () => {
		expect(fits('png', 600, 595, 2000)).toBe(true);
		expect(fits('png', 2400, 595, 842)).toBe(false);
		expect(fits('jpeg', 300, 72, 16_000)).toBe(false);
		expect(fits('tiff', 300, 72, 16_000)).toBe(true);
	});

	it('says which page is too large and the most resolution that fits', () => {
		const sizes = [letter, { width: 612, height: 1008 }, letter];
		expect(tooLarge('png', 1100, [0, 1, 2], sizes)).toBeNull();
		const found = tooLarge('png', 1200, [0, 1, 2], sizes);
		expect(found?.page).toBe(1);
		expect(found?.maxDpi).toBeGreaterThan(1100);
		expect(fits('png', found!.maxDpi!, 612, 1008)).toBe(true);
		expect(fits('png', found!.maxDpi! + 1, 612, 1008)).toBe(false);
		const px = pixelSize(612, 1008, 1200);
		expect(tooLargeText(found!, 'png', labels)).toBe(
			`Page ii would be ${px.width.toLocaleString()} × ${px.height.toLocaleString()} pixels, too large for PNG. Choose ${found!.maxDpi} dpi or less.`
		);
		// Pages left out of the export don't count.
		expect(tooLarge('png', 1200, [0, 2], sizes)).toBeNull();
	});
});

describe('the options of each format', () => {
	it('black and white only where it is kept as one bit a pixel', () => {
		expect(colours('png').map((c) => c.value)).toContain('blackAndWhite');
		expect(colours('tiff').map((c) => c.value)).toContain('blackAndWhite');
		expect(colours('jpeg').map((c) => c.value)).not.toContain('blackAndWhite');
	});

	it('a quality for JPEG alone', () => {
		expect(hasQuality('jpeg')).toBe(true);
		expect(hasQuality('png')).toBe(false);
		expect(hasQuality('tiff')).toBe(false);
	});
});

describe('names and messages', () => {
	it('names files by number or label, as Rust does', () => {
		expect(exampleName('report', 6, 120, 'numbers', null, 'png')).toBe('report-007.png');
		expect(exampleName('report', 6, 120, 'labels', labels, 'jpeg')).toBe('report-A_1.jpg');
		expect(exampleName('report', 0, 9, 'labels', null, 'tiff')).toBe('report-1.tif');
		expect(safeLabel('a?/b. ')).toBe('a__b');
		expect(stemOf('report.final.pdf')).toBe('report.final');
		expect(stemOf('.hidden')).toBe('.hidden');
	});

	it('says which files are there already', () => {
		const target = { folder: 'Pictures', first: 'r-01.png', last: 'r-12.png', files: 12 };
		expect(existingText({ ...target, existing: ['r-01.png'], existingCount: 1 })).toBe(
			'1 of these 12 files is already in Pictures: r-01.png.'
		);
		expect(existingText({ ...target, existing: ['r-01.png', 'r-02.png'], existingCount: 2 })).toBe(
			'2 of these 12 files are already in Pictures: r-01.png and r-02.png.'
		);
		expect(
			existingText({ ...target, existing: ['r-01.png', 'r-02.png', 'r-03.png'], existingCount: 12 })
		).toBe('All 12 files are already in Pictures: r-01.png, r-02.png, r-03.png and 9 more.');
	});

	it('says what was exported', () => {
		const done = { pages: 12, files: 12, folder: 'Pictures', first: 'r-01.png', token: 1 };
		expect(doneMessage('r.pdf', done, 'jpeg')).toBe('Exported 12 pages of r.pdf as JPEG images to Pictures.');
		expect(doneMessage('r.pdf', { ...done, files: 1, first: 'r.tif' }, 'tiff')).toBe(
			'Exported 12 pages of r.pdf to r.tif in Pictures.'
		);
		expect(doneMessage('r.pdf', { ...done, pages: 1, files: 1, first: 'r-04.png' }, 'png')).toBe(
			'Exported a page of r.pdf to r-04.png in Pictures.'
		);
		expect(doneMessage('r.pdf', { ...done, pages: 0, files: 0 }, 'png')).toMatch(/^Nothing was exported/);
	});
});

describe('copying a page as an image', () => {
	it('says which page, and the resolution when it was lower than asked', () => {
		expect(copiedMessage('iv', 150, 150)).toBe('Copied page iv as an image.');
		expect(copiedMessage('3', 212, 300)).toBe('Copied page 3 as an image, at 212 dpi as the page is large.');
	});
});
