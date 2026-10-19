// What the document's properties show (section 6.10): sizes, the page size with its paper,
// fonts and what a protection allows, in words.

import type { DocumentDescription, FontKind, ProtectionInfo, UsedFont } from '#lib/ipc/index.ts';
import { fileSize } from '#lib/format.ts';

import { CHANGES } from '../protect/protect.ts';

/** "2.4 MB (2,503,112 bytes)". */
export function sizeText(bytes: number, locale?: string): string {
	const exact = `${bytes.toLocaleString(locale)} ${bytes === 1 ? 'byte' : 'bytes'}`;
	return bytes < 1024 ? exact : `${fileSize(bytes, locale)} (${exact})`;
}

const PAPERS: readonly { name: string; width: number; height: number }[] = [
	{ name: 'A3', width: 297, height: 420 },
	{ name: 'A4', width: 210, height: 297 },
	{ name: 'A5', width: 148, height: 210 },
	{ name: 'B5', width: 176, height: 250 },
	{ name: 'Letter', width: 215.9, height: 279.4 },
	{ name: 'Legal', width: 215.9, height: 355.6 },
	{ name: 'Tabloid', width: 279.4, height: 431.8 }
];

const MM_PER_POINT = 25.4 / 72;

/** Systems that measure in inches. */
export function usesInches(locale: string): boolean {
	return /^(en|es)-US$/i.test(locale);
}

/** A page of `width` × `height` points: "210 × 297 mm (A4)", or in inches. */
export function pageSizeText(width: number, height: number, inches: boolean, locale?: string): string {
	const w = width * MM_PER_POINT;
	const h = height * MM_PER_POINT;
	const near = (a: number, b: number) => Math.abs(a - b) <= 2;
	const paper = PAPERS.find((p) => (near(w, p.width) && near(h, p.height)) || (near(w, p.height) && near(h, p.width)));
	const number = (n: number, digits: number) => n.toLocaleString(locale, { maximumFractionDigits: digits });
	const size = inches
		? `${number(width / 72, 2)} × ${number(height / 72, 2)} in`
		: `${number(w, 0)} × ${number(h, 0)} mm`;
	return paper ? `${size} (${paper.name})` : size;
}

const FONT_KINDS: Record<FontKind, string> = {
	type1: 'Type 1',
	type1Compact: 'Type 1 (CFF)',
	multipleMaster: 'Type 1 (Multiple Master)',
	trueType: 'TrueType',
	openType: 'OpenType',
	type3: 'Type 3',
	cidType0: 'Type 0 (CID, CFF)',
	cidTrueType: 'Type 0 (CID, TrueType)',
	unknown: 'Unknown'
};

export function fontKindText(kind: FontKind): string {
	return FONT_KINDS[kind];
}

/** Whether its glyphs are in the file, in words. */
export function embeddingText(font: UsedFont): string {
	if (font.embedded) return font.subset ? 'Embedded subset' : 'Embedded';
	return font.standard ? 'Not embedded (standard font)' : 'Not embedded';
}

/** A font not embedded, which readers draw with a font of their own: worth a mark. The 14
 * standard fonts every reader has are not. */
export function fontNeedsAttention(font: UsedFont): boolean {
	return !font.embedded && !font.standard;
}

const ENCODINGS: Record<string, string> = {
	WinAnsiEncoding: 'Windows (ANSI)',
	MacRomanEncoding: 'Mac Roman',
	MacExpertEncoding: 'Mac Expert',
	StandardEncoding: 'Standard',
	PDFDocEncoding: 'PDFDoc',
	'Identity-H': 'Identity-H (Unicode)',
	'Identity-V': 'Identity-V (Unicode)'
};

export function encodingText(encoding: string | null): string {
	if (encoding === null) return 'Built-in';
	return ENCODINGS[encoding] ?? encoding;
}

/** Fonts found in another part of the document added to those found already: each once
 * (by object number; fonts written into their resources, by name). */
export function mergeFonts(found: readonly UsedFont[], more: readonly UsedFont[]): UsedFont[] {
	const key = (f: UsedFont) => (f.id === 0 ? `name:${f.name}` : `id:${f.id}`);
	const seen = new Set(found.map(key));
	const merged = [...found];
	for (const font of more) {
		if (seen.has(key(font))) continue;
		seen.add(key(font));
		merged.push(font);
	}
	return merged;
}

/** What a protection allows, row by row, as the Security tab lists it. */
export function allowedRows(protection: ProtectionInfo): { label: string; value: string }[] {
	const a = protection.allowed;
	const yes = (allowed: boolean) => (allowed ? 'Allowed' : 'Not allowed');
	return [
		{
			label: 'Printing',
			value: a.printHighResolution ? 'High resolution' : a.print ? 'Low resolution' : 'Not allowed'
		},
		{ label: 'Changes', value: CHANGES.find((c) => c.value === protection.restrictions.changes)?.label ?? '' },
		{ label: 'Copying text and images', value: yes(a.copy) },
		{ label: 'Commenting', value: yes(a.annotate) },
		{ label: 'Filling in forms', value: yes(a.fillForms) },
		{ label: 'Inserting, deleting and rotating pages', value: yes(a.assemble || a.modify) }
	];
}

/** "AES 256-bit". */
export function cipherText(protection: ProtectionInfo): string {
	const name = protection.cipher === 'aes' ? 'AES' : protection.cipher === 'rc4' ? 'RC4' : 'Unknown';
	return protection.keyBits > 0 ? `${name} ${protection.keyBits}-bit` : name;
}

/** The description with each value's ends trimmed, as it is saved. */
export function trimmed(d: DocumentDescription): DocumentDescription {
	return { title: d.title.trim(), author: d.author.trim(), subject: d.subject.trim(), keywords: d.keywords.trim() };
}

export function sameDescription(a: DocumentDescription, b: DocumentDescription): boolean {
	const x = trimmed(a);
	const y = trimmed(b);
	return x.title === y.title && x.author === y.author && x.subject === y.subject && x.keywords === y.keywords;
}
