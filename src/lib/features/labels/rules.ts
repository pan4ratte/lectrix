// Page label rules as the Page labels dialog changes them (section 6.3): the pages picked
// there are numbered anew, and the rules that say so are worked out here. Formatting
// mirrors crates/pdf-core/src/labels.rs so the live preview shows what Rust will write;
// once a change is applied, the labels Rust returns replace the preview.

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
 * The rules a change starts from, for the stored ones: sorted, those past the last page
 * left out (they have no effect; `hidden` counts them), and a decimal rule at the first
 * page if the file has none (that is how its pages are numbered, and Rust adds it on the
 * next change anyway).
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


/** The rule that, starting at `page`, gives it and the pages after it the labels they
 * have under `rules`. */
export function ruleFrom(rules: readonly LabelRule[], page: number): LabelRule {
	const rule = rules.findLast((r) => r.startPage <= page) ?? defaultRule();
	return {
		startPage: page,
		style: rule.style,
		prefix: rule.prefix,
		firstNumber: Math.min(rule.firstNumber + (page - rule.startPage), MAX_FIRST_NUMBER)
	};
}

/** How the Page labels dialog numbers pages, as Acrobat's does: a new section with its own
 * style, prefix and first number, or the numbering of the page before, carried on. */
export type Numbering =
	| { kind: 'new'; style: LabelStyle; prefix: string; firstNumber: number }
	| { kind: 'continue' };

/** Why a new section can't be numbered so, or null. */
export function numberingProblem(numbering: Numbering, pages: number): string | null {
	if (numbering.kind !== 'new' || numbering.style === 'none') return null;
	const first = numbering.firstNumber;
	if (!Number.isInteger(first) || first < 1) return 'Numbers start at 1 or higher.';
	if (first + pages - 1 > MAX_FIRST_NUMBER) return 'That number is too large.';
	return null;
}

/** `pages` (sorted) as runs of pages next to each other: each from `start` up to, not
 * including, `end`. */
export function runsOf(pages: readonly number[]): { start: number; end: number }[] {
	const runs: { start: number; end: number }[] = [];
	for (const page of pages) {
		const last = runs.at(-1);
		if (last && last.end === page) last.end = page + 1;
		else runs.push({ start: page, end: page + 1 });
	}
	return runs;
}

const sameLabels = (a: LabelRule, b: LabelRule) =>
	a.style === b.style && a.prefix === b.prefix && (a.style === 'none' || a.firstNumber === b.firstNumber);

/**
 * `rules` (as `editableRules` gives them) with `pages` (0-based, sorted, each once)
 * numbered as `numbering` says. The pages are numbered through in their order, also
 * across pages that are not among them, and every other page keeps the label it has: where
 * a run of the pages ends, a rule gives the page after it its old label back.
 * Carrying on the numbering of the page before takes the rules inside the pages away; the
 * first page of the document has no page before it and is numbered 1, 2, 3.
 * Rules this adds that only repeat what the rule before them says are left out.
 */
export function assignLabels(
	rules: readonly LabelRule[],
	pageCount: number,
	pages: readonly number[],
	numbering: Numbering
): LabelRule[] {
	let next = [...rules];
	let done = 0;
	for (const run of runsOf(pages)) {
		const after = run.end < pageCount ? ruleFrom(rules, run.end) : null;
		next = next.filter((r) => r.startPage < run.start || r.startPage >= run.end);
		if (numbering.kind === 'new') {
			const { style, prefix, firstNumber } = numbering;
			next.push({ startPage: run.start, style, prefix, firstNumber: style === 'none' ? 1 : firstNumber + done });
		} else if (run.start === 0) {
			next.push(defaultRule());
		}
		if (after && !next.some((r) => r.startPage === run.end)) next.push(after);
		next.sort((a, b) => a.startPage - b.startPage);
		done += run.end - run.start;
	}
	const stored = (rule: LabelRule) => rules.some((r) => r.startPage === rule.startPage && sameLabels(r, rule));
	const out: LabelRule[] = [];
	for (const rule of next) {
		const before = out.at(-1);
		if (before && !stored(rule) && sameLabels(ruleFrom([before], rule.startPage), rule)) continue;
		out.push(rule);
	}
	return out;
}

/** Pages (0-based, sorted) as the dialog names them: "2–3, 7", cut short after a few runs. */
export function pagesText(pages: readonly number[]): string {
	const runs = runsOf(pages).map((r) => (r.end - r.start === 1 ? String(r.start + 1) : `${r.start + 1}–${r.end}`));
	return runs.length > 4 ? `${runs.slice(0, 3).join(', ')}, …` : runs.join(', ');
}

/** The labels `pages` get, as the dialog previews them: "i, ii, iii", or the first three
 * and the last of more ("i, ii, iii … xii"). */
export function previewText(labels: readonly string[], pages: readonly number[]): string {
	const of = (page: number) => labels[page] || '(no label)';
	if (pages.length <= 4) return pages.map(of).join(', ');
	return `${pages.slice(0, 3).map(of).join(', ')} … ${of(pages[pages.length - 1]!)}`;
}
