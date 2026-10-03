// Page label rules as the panel edits them (section 6.3). Formatting mirrors
// crates/pdf-core/src/labels.rs so the live preview shows what Rust will write; once a
// change is applied, the labels Rust returns replace the preview.

import type { LabelRule, LabelStyle } from '#lib/ipc/index.ts';

/** Same limit as Rust's MAX_LABEL_CHARS. */
export const MAX_LABEL_CHARS = 256;
/** `/St` is a PDF integer. */
export const MAX_FIRST_NUMBER = 2_147_483_647;

export const STYLES: { value: LabelStyle; name: string }[] = [
	{ value: 'decimal', name: '1, 2, 3' },
	{ value: 'lowerRoman', name: 'i, ii, iii' },
	{ value: 'upperRoman', name: 'I, II, III' },
	{ value: 'lowerLetters', name: 'a, b, c' },
	{ value: 'upperLetters', name: 'A, B, C' },
	{ value: 'none', name: 'None (prefix only)' }
];

const ROMAN: [number, string][] = [
	[1000, 'M'],
	[900, 'CM'],
	[500, 'D'],
	[400, 'CD'],
	[100, 'C'],
	[90, 'XC'],
	[50, 'L'],
	[40, 'XL'],
	[10, 'X'],
	[9, 'IX'],
	[5, 'V'],
	[4, 'IV'],
	[1, 'I']
];

function roman(n: number): string {
	let out = '';
	for (const [value, digits] of ROMAN) {
		while (n >= value) {
			out += digits;
			n -= value;
		}
	}
	return out;
}

/** PDF letter numbering: a…z, then aa…zz, then aaa…zzz (one letter repeated). */
function letters(n: number, base: string): string {
	if (n < 1) return '';
	const letter = String.fromCharCode(base.charCodeAt(0) + ((n - 1) % 26));
	return letter.repeat(Math.floor((n - 1) / 26) + 1);
}

/** Formats `n` (1 or higher) in `style`; overlong roman numerals and letters fall back to decimal. */
export function formatNumber(n: number, style: LabelStyle): string {
	const tooLong =
		style === 'lowerRoman' || style === 'upperRoman'
			? Math.floor(n / 1000) + 12 > MAX_LABEL_CHARS
			: style === 'lowerLetters' || style === 'upperLetters'
				? Math.floor((n - 1) / 26) >= MAX_LABEL_CHARS
				: false;
	if (tooLong) return String(n);
	switch (style) {
		case 'none':
			return '';
		case 'decimal':
			return String(n);
		case 'upperRoman':
			return roman(n);
		case 'lowerRoman':
			return roman(n).toLowerCase();
		case 'upperLetters':
			return letters(n, 'A');
		case 'lowerLetters':
			return letters(n, 'a');
	}
}

/** Every page's label under `rules` (sorted by start page). */
export function labelsForPages(rules: readonly LabelRule[], pageCount: number): string[] {
	const out: string[] = new Array<string>(pageCount);
	let r = -1;
	for (let page = 0; page < pageCount; page++) {
		while (r + 1 < rules.length && rules[r + 1]!.startPage <= page) r++;
		const rule = rules[r];
		if (!rule) {
			out[page] = String(page + 1);
			continue;
		}
		const n = Math.min(rule.firstNumber + (page - rule.startPage), 4_294_967_295);
		const label = rule.prefix + formatNumber(n, rule.style);
		out[page] = [...label].length > MAX_LABEL_CHARS ? [...label].slice(0, MAX_LABEL_CHARS).join('') : label;
	}
	return out;
}

export const defaultRule = (startPage = 0): LabelRule => ({
	startPage,
	style: 'decimal',
	prefix: '',
	firstNumber: 1
});

/**
 * The rules the panel shows for the stored ones: sorted, those past the last page left out
 * (they have no effect; `hidden` counts them), and a decimal rule at the first page if
 * the file has none (that is how its pages are numbered, and Rust adds it on the next
 * change anyway).
 */
export function editableRules(
	stored: readonly LabelRule[],
	pageCount: number
): { rules: LabelRule[]; hidden: number } {
	const sorted = [...stored].sort((a, b) => a.startPage - b.startPage);
	const rules: LabelRule[] = [];
	let hidden = 0;
	for (const rule of sorted) {
		if (rule.startPage >= pageCount) hidden++;
		else if (rules.at(-1)?.startPage !== rule.startPage) rules.push({ ...rule });
	}
	if (rules[0]?.startPage !== 0) rules.unshift(defaultRule());
	return { rules, hidden };
}

export function sameRules(a: readonly LabelRule[], b: readonly LabelRule[]): boolean {
	return (
		a.length === b.length &&
		a.every(
			(r, i) =>
				r.startPage === b[i]!.startPage &&
				r.style === b[i]!.style &&
				r.prefix === b[i]!.prefix &&
				r.firstNumber === b[i]!.firstNumber
		)
	);
}

/** Page index after the last page of each rule's range. */
export function rangeEnds(rules: readonly LabelRule[], pageCount: number): number[] {
	return rules.map((_, i) => rules[i + 1]?.startPage ?? pageCount);
}

/** Why a rule can't be used in `rules` (where it replaces the rule at `index`), or null. */
export function ruleProblem(
	rule: LabelRule,
	rules: readonly LabelRule[],
	index: number,
	pageCount: number
): string | null {
	if (!Number.isInteger(rule.startPage) || rule.startPage < 0 || rule.startPage >= pageCount) {
		return `The start page must be between 1 and ${pageCount}.`;
	}
	if (index === 0 && rule.startPage !== 0) return 'The first rule always starts at page 1.';
	if (index !== 0 && rule.startPage === 0) return 'Page 1 already has a rule; change that one instead.';
	if (rules.some((r, i) => i !== index && r.startPage === rule.startPage)) {
		return `Another rule already starts at page ${rule.startPage + 1}.`;
	}
	if (!Number.isInteger(rule.firstNumber) || rule.firstNumber < 1) return 'Numbers start at 1 or higher.';
	if (rule.firstNumber > MAX_FIRST_NUMBER) return 'That number is too large.';
	return null;
}

/** `rules` with the rule at `index` replaced, sorted again by start page. */
export function replaceRule(rules: readonly LabelRule[], index: number, rule: LabelRule): LabelRule[] {
	return rules.map((r, i) => (i === index ? rule : r)).sort((a, b) => a.startPage - b.startPage);
}

/**
 * `rules` with a new range starting at `page`, numbered 1, 2, 3 from 1 (as Acrobat's
 * "Begin new section"). If a rule already starts there, nothing changes.
 */
export function withRuleAt(rules: readonly LabelRule[], page: number): LabelRule[] {
	if (rules.some((r) => r.startPage === page)) return [...rules];
	return [...rules, defaultRule(page)].sort((a, b) => a.startPage - b.startPage);
}

/** "Roman front matter, then arabic from this page." */
export function romanThenArabic(page: number): LabelRule[] {
	return [{ ...defaultRule(0), style: 'lowerRoman' }, defaultRule(page)];
}

/** "i – xviii", or one label for a one-page range. */
export function rangeText(labels: readonly string[], start: number, end: number): string {
	const first = labels[start] || '(no label)';
	if (end - start <= 1) return first;
	const last = labels[end - 1] || '(no label)';
	return `${first} – ${last}`;
}

/** "1–4", or "12" for one page. */
export function pagesText(start: number, end: number): string {
	return end - start <= 1 ? String(start + 1) : `${start + 1}–${end}`;
}
