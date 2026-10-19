import { describe, expect, it } from 'vitest';

import type { CompressAnalysis, CompressOutcome, CompressPresets, CompressSettings, ImageSettings } from '#lib/ipc/index.ts';

import {
	compressedMessage,
	details,
	downsampleProblem,
	formatSize,
	headline,
	matchingPreset,
	presetSettings,
	removalRows,
	renamed,
	savedPercent,
	settingsValid,
	simpleMono,
	simplified,
	sizeParts,
	stageText,
	stageValue
} from './compress.ts';

// The presets as Rust makes them (ipc.rs, `CompressSettings::of_preset`; its test checks
// that black-and-white images follow `simpleMono` there too).
function images(toDpi: number | null, quality: number): ImageSettings {
	return {
		downsample: {
			enabled: toDpi !== null,
			toDpi: toDpi ?? 150,
			aboveDpi: Math.floor(((toDpi ?? 150) * 3) / 2),
			method: 'bicubic'
		},
		compression: toDpi === null ? 'keep' : 'jpeg',
		quality
	};
}

function preset(name: CompressSettings['preset'], toDpi: number | null, quality: number, mono: number | null): CompressSettings {
	return {
		preset: name,
		separate: false,
		color: images(toDpi, quality),
		gray: images(toDpi, quality),
		mono: {
			enabled: mono !== null,
			toDpi: mono ?? 300,
			aboveDpi: Math.floor(((mono ?? 300) * 3) / 2),
			method: 'bicubic'
		},
		colorMode: 'keep',
		subsetFonts: true,
		objectStreams: true,
		remove: { thumbnails: true, privateData: true, attachments: false, javascript: false, tags: false }
	};
}

const PRESETS: CompressPresets = {
	lossless: preset('lossless', null, 85, null),
	highQuality: preset('highQuality', 300, 85, 600),
	balanced: preset('balanced', 150, 75, 300),
	smallest: preset('smallest', 96, 50, 300)
};

describe('presets', () => {
	it('are named back from their settings', () => {
		for (const name of ['lossless', 'highQuality', 'balanced', 'smallest'] as const) {
			const settings = { ...presetSettings(PRESETS, name), preset: 'custom' as const, separate: true };
			expect(matchingPreset(settings, PRESETS)).toBe(name);
		}
	});

	it('become Custom once a setting differs', () => {
		const s = PRESETS.balanced;
		expect(renamed({ ...s, color: { ...s.color, quality: 76 } }, PRESETS).preset).toBe('custom');
		expect(renamed({ ...s, remove: { ...s.remove, tags: true } }, PRESETS).preset).toBe('custom');
		expect(renamed({ ...s, colorMode: 'gray' }, PRESETS).preset).toBe('custom');
	});

	it('are what the simple form makes of their colour settings', () => {
		for (const s of Object.values(PRESETS)) expect(simplified(s)).toEqual(s);
	});
});

describe('the simple form', () => {
	it('gives greyscale images the colour settings and black and white at least 300 dpi', () => {
		const s = PRESETS.balanced;
		const custom = simplified({ ...s, color: { ...s.color, quality: 60, downsample: { ...s.color.downsample, toDpi: 200, aboveDpi: 250 } } });
		expect(custom.gray).toEqual(custom.color);
		expect(custom.mono).toEqual({ enabled: true, toDpi: 400, aboveDpi: 600, method: 'bicubic' });
		expect(simpleMono({ enabled: true, toDpi: 72, aboveDpi: 100, method: 'average' })).toEqual({
			enabled: true,
			toDpi: 300,
			aboveDpi: 450,
			method: 'average'
		});
	});
});

describe('downsampling numbers', () => {
	it('are checked only when downsampling is on', () => {
		expect(downsampleProblem({ enabled: false, toDpi: 0, aboveDpi: 0, method: 'bicubic' })).toBeNull();
		expect(downsampleProblem({ enabled: true, toDpi: 150, aboveDpi: 225, method: 'bicubic' })).toBeNull();
		expect(downsampleProblem({ enabled: true, toDpi: 150, aboveDpi: 150, method: 'bicubic' })).toBeNull();
		expect(downsampleProblem({ enabled: true, toDpi: 8, aboveDpi: 225, method: 'bicubic' })).toMatch(/9 to 2400/);
		expect(downsampleProblem({ enabled: true, toDpi: 150.5, aboveDpi: 225, method: 'bicubic' })).toMatch(/9 to 2400/);
		expect(downsampleProblem({ enabled: true, toDpi: 300, aboveDpi: 200, method: 'bicubic' })).toMatch(/can’t be lower/);
	});

	it('and quality make settings valid or not', () => {
		expect(settingsValid(PRESETS.balanced)).toBe(true);
		const s = PRESETS.balanced;
		expect(settingsValid({ ...s, gray: { ...s.gray, quality: 0 } })).toBe(false);
		expect(settingsValid({ ...s, mono: { ...s.mono, toDpi: 5000 } })).toBe(false);
	});
});

describe('sizes', () => {
	it('are written as the app writes sizes', () => {
		expect(formatSize(512, 'en-US')).toBe('512 bytes');
		expect(formatSize(820 * 1024, 'en-US')).toBe('820 KB');
		expect(formatSize(6.13 * 1024 * 1024, 'en-US')).toBe('6.1 MB');
		expect(formatSize(48.23 * 1024 * 1024, 'en-US')).toBe('48 MB');
	});

	it('say how much smaller', () => {
		expect(savedPercent(1000, 130)).toBe('87%');
		expect(savedPercent(1000, 998)).toBe('less than 1%');
		expect(savedPercent(1000, 1200)).toBe('0%');
	});
});

const usage = (bytes: number, count = 1) => ({ bytes, count });
const ANALYSIS: CompressAnalysis = {
	fileSize: 10_000_000,
	pages: 12,
	images: usage(7_000_000, 30),
	fonts: usage(1_000_000, 4),
	content: usage(500_000, 12),
	attachments: usage(0, 0),
	thumbnails: usage(120_000, 12),
	privateData: usage(0, 0),
	metadata: usage(5_000),
	other: 1_375_000,
	found: { thumbnails: 12, privateData: 0, attachments: 0, javascript: 3, tags: true }
};

describe('the analysis', () => {
	it('shows the parts the file has, largest first, the rest last', () => {
		expect(sizeParts(ANALYSIS).map((p) => [p.label, p.bytes])).toEqual([
			['Images', 7_000_000],
			['Fonts', 1_000_000],
			['Page content', 500_000],
			['Thumbnails', 120_000],
			['Other', 1_380_000]
		]);
	});

	it('offers to remove only what the file has', () => {
		const rows = removalRows(ANALYSIS);
		expect(rows.map((r) => r.key)).toEqual(['thumbnails', 'javascript', 'tags']);
		expect(rows[0]?.detail).toBe('12 pages, 117 KB. Readers draw their own.');
		expect(rows[1]?.detail).toMatch(/^3 scripts\. Forms/);
	});
});

describe('progress', () => {
	it('names each stage and how far it is', () => {
		expect(stageText({ stage: 'reading', done: 3, total: 120 })).toBe('Reading pages… 3 of 120');
		expect(stageText({ stage: 'images', done: 1, total: 4 })).toBe('Compressing images… 1 of 4');
		expect(stageText({ stage: 'fonts', done: 0, total: 0 })).toBe('Subsetting fonts…');
		expect(stageValue({ stage: 'images', done: 1, total: 4 })).toBe(25);
		expect(stageValue({ stage: 'writing', done: 0, total: 0 })).toBeNull();
	});
});

const OUTCOME: CompressOutcome = {
	before: 48 * 1024 * 1024,
	after: 6 * 1024 * 1024,
	images: 410,
	imagesReplaced: 380,
	imagesDownsampled: 212,
	imagesSkipped: 6,
	fontsSubset: 1,
	removed: { thumbnails: 312, privateData: 0, attachments: 0, javascript: 0, tags: false },
	previewPage: 4,
	pages: 312
};

describe('the result', () => {
	it('says what was done', () => {
		expect(headline(OUTCOME)).toBe('48 MB → 6 MB, 88% smaller');
		expect(details(OUTCOME)).toEqual([
			'410 images: 380 recompressed, 212 downsampled. 6 left as they were.',
			'1 font subset.',
			'Removed: 312 thumbnails.'
		]);
	});

	it('is said in a notification once saved', () => {
		expect(compressedMessage('a.pdf', true, 2048 * 1024, 1024 * 1024).message).toBe('a.pdf is now 1 MB (it was 2 MB).');
		const copy = compressedMessage('a (compressed).pdf', false, 2048 * 1024, 1024 * 1024);
		expect(copy.message).toBe('Saved a (compressed).pdf: 1 MB, 50% smaller.');
		expect(copy.suggestion).toMatch(/original/);
	});
});
