import { describe, expect, it } from 'vitest';

import type { FoundImage, ImagesFound } from '#lib/ipc/index.ts';

import { exampleImageName, imageTip, imagesDoneMessage, leftOutText, noneText, summaryText } from './images.ts';

function image(page: number, index: number, kind: FoundImage['kind'], bytes = 1000): FoundImage {
	return { page, index, num: 10 + page, key: `k${page}-${index}`, width: 1200, height: 800, kind, bytes };
}

function found(images: FoundImage[], repeats = 0, small = 0): ImagesFound {
	return { images, repeats, small };
}

describe('what the dialog says of the images', () => {
	it('counts them by kind, with their size, about it if any is PNG', () => {
		const jpegs = [image(0, 1, 'jpeg', 1_000_000), image(1, 1, 'jpeg', 500_000)];
		expect(summaryText(found(jpegs))).toMatch(/^2 images: 2 JPEG, 1\D\d MB\.$/);
		const mixed = [...jpegs, image(1, 2, 'png'), image(2, 1, 'jpeg2000')];
		expect(summaryText(found(mixed))).toMatch(/^4 images: 2 JPEG, 1 JPEG 2000 and 1 PNG, about \S+ MB\.$/);
		expect(summaryText(found([image(0, 1, 'png')]))).toMatch(/^1 image: 1 PNG, about /);
		expect(summaryText(found([]))).toBe('');
	});

	it('says what is left out', () => {
		expect(leftOutText(found([], 0, 0), 32)).toBe('');
		expect(leftOutText(found([], 1, 0), 32)).toBe('Left out: an image drawn again, or alike, exported once.');
		expect(leftOutText(found([], 19, 5), 32)).toBe(
			'Left out: 19 images drawn again, or alike, exported once and 5 images smaller than 32 pixels.'
		);
		expect(leftOutText(found([], 0, 1), 64)).toBe('Left out: 1 image smaller than 64 pixels.');
		// Small images aren't left out when the box is cleared.
		expect(leftOutText(found([], 0, 3), null)).toBe('');
	});

	it('says why there is nothing to export', () => {
		expect(noneText(found([]), 32, 12)).toBe('There are no images on these pages.');
		expect(noneText(found([]), 32, 1)).toBe('There are no images on this page.');
		expect(noneText(found([], 0, 2), 32, 12)).toBe('Every image on these pages is smaller than 32 pixels.');
	});

	it('names an example file as Rust does', () => {
		const labels = ['i', 'ii', 'A:1'];
		expect(exampleImageName('report', image(6, 2, 'jpeg'), 120, 3, 'numbers', null)).toBe('report-007-2.jpg');
		expect(exampleImageName('report', image(2, 1, 'png'), 3, 12, 'labels', labels)).toBe('report-A_1-01.png');
		expect(exampleImageName('report', image(0, 1, 'jpeg2000'), 3, 1, 'labels', labels)).toBe('report-i-1.jp2');
	});

	it('tells each thumbnail what it is', () => {
		const px = `${(1200).toLocaleString()} × 800 pixels`;
		expect(imageTip(image(6, 1, 'jpeg', 2048), 'vii')).toBe(`Page vii · ${px} · JPEG, 2 KB`);
		expect(imageTip(image(0, 1, 'png', 2048), '1')).toBe(`Page 1 · ${px} · PNG, about 2 KB`);
	});
});

describe('the notification', () => {
	const done = { files: 12, unreadable: 0, folder: 'Pictures', first: 'report-001-1.jpg', token: 3 };

	it('says how many images went where', () => {
		expect(imagesDoneMessage('report.pdf', done)).toBe('Exported 12 images from report.pdf to Pictures.');
		expect(imagesDoneMessage('report.pdf', { ...done, files: 1 })).toBe(
			'Exported an image of report.pdf to report-001-1.jpg in Pictures.'
		);
		expect(imagesDoneMessage('report.pdf', { ...done, files: 0 })).toBe(
			'Nothing was exported: every file was already in its folder.'
		);
	});

	it('says how many could not be read', () => {
		expect(imagesDoneMessage('report.pdf', { ...done, unreadable: 1 })).toBe(
			'Exported 12 images from report.pdf to Pictures. 1 image couldn’t be read and was left out.'
		);
		expect(imagesDoneMessage('report.pdf', { ...done, unreadable: 2 })).toBe(
			'Exported 12 images from report.pdf to Pictures. 2 images couldn’t be read and were left out.'
		);
		expect(imagesDoneMessage('report.pdf', { ...done, files: 0, unreadable: 2 })).toBe(
			'Nothing was exported: the images couldn’t be read.'
		);
	});
});
