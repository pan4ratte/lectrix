import { describe, expect, it } from 'vitest';

import type { ProtectionInfo, UsedFont } from '#lib/ipc/index.ts';

import {
	allowedRows,
	cipherText,
	embeddingText,
	encodingText,
	fontNeedsAttention,
	mergeFonts,
	pageSizeText,
	sameDescription,
	sizeText,
	usesInches
} from './properties.ts';

const font = (changes: Partial<UsedFont>): UsedFont => ({
	id: 5,
	name: 'Helvetica',
	subset: false,
	kind: 'type1',
	embedded: false,
	standard: true,
	encoding: 'WinAnsiEncoding',
	firstPage: 0,
	...changes
});

describe('sizes', () => {
	it('gives the size and the exact count', () => {
		expect(sizeText(2_503_112, 'en-US')).toBe('2.4 MB (2,503,112 bytes)');
		expect(sizeText(900, 'en-US')).toBe('900 bytes');
		expect(sizeText(1, 'en-US')).toBe('1 byte');
	});

	it('names standard papers either way round, in millimetres or inches', () => {
		expect(pageSizeText(595, 842, false, 'en-GB')).toBe('210 × 297 mm (A4)');
		expect(pageSizeText(842, 595, false, 'en-GB')).toBe('297 × 210 mm (A4)');
		expect(pageSizeText(612, 792, true, 'en-US')).toBe('8.5 × 11 in (Letter)');
		expect(pageSizeText(500, 500, false, 'en-GB')).toBe('176 × 176 mm');
		expect(usesInches('en-US')).toBe(true);
		expect(usesInches('en-GB')).toBe(false);
	});
});

describe('fonts', () => {
	it('says whether a font is embedded, and marks only those readers replace', () => {
		expect(embeddingText(font({}))).toBe('Not embedded (standard font)');
		expect(fontNeedsAttention(font({}))).toBe(false);
		const missing = font({ name: 'Calibri', kind: 'trueType', standard: false });
		expect(embeddingText(missing)).toBe('Not embedded');
		expect(fontNeedsAttention(missing)).toBe(true);
		expect(embeddingText(font({ embedded: true, subset: true, standard: false }))).toBe('Embedded subset');
	});

	it('names encodings', () => {
		expect(encodingText('WinAnsiEncoding')).toBe('Windows (ANSI)');
		expect(encodingText(null)).toBe('Built-in');
		expect(encodingText('Custom')).toBe('Custom');
	});

	it('lists each font once across parts of the document', () => {
		const first = [font({ id: 5 }), font({ id: 0, name: 'Inline' })];
		const more = [font({ id: 5, firstPage: 9 }), font({ id: 0, name: 'Inline' }), font({ id: 7, name: 'Times-Roman' })];
		const merged = mergeFonts(first, more);
		expect(merged.map((f) => f.name)).toEqual(['Helvetica', 'Inline', 'Times-Roman']);
		expect(merged[0]!.firstPage).toBe(0);
	});
});

describe('security', () => {
	const protection: ProtectionInfo = {
		openPassword: false,
		restricted: true,
		allowed: {
			print: true,
			printHighResolution: false,
			modify: false,
			copy: false,
			annotate: true,
			fillForms: true,
			assemble: false
		},
		restrictions: { printing: 'lowResolution', changes: 'commentsFormsAndSigning', copying: false },
		cipher: 'aes',
		keyBits: 256,
		unrestricted: false
	};

	it('lists what the file allows', () => {
		expect(allowedRows(protection)).toEqual([
			{ label: 'Printing', value: 'Low resolution' },
			{ label: 'Changes', value: 'Commenting, filling in forms and signing' },
			{ label: 'Copying text and images', value: 'Not allowed' },
			{ label: 'Commenting', value: 'Allowed' },
			{ label: 'Filling in forms', value: 'Allowed' },
			{ label: 'Inserting, deleting and rotating pages', value: 'Not allowed' }
		]);
		expect(cipherText(protection)).toBe('AES 256-bit');
		expect(cipherText({ ...protection, cipher: 'rc4', keyBits: 40 })).toBe('RC4 40-bit');
	});
});

describe('description', () => {
	it('compares as saved, ends trimmed', () => {
		const d = { title: 'A', author: '', subject: '', keywords: '' };
		expect(sameDescription(d, { ...d, title: ' A ' })).toBe(true);
		expect(sameDescription(d, { ...d, author: 'B' })).toBe(false);
	});
});
