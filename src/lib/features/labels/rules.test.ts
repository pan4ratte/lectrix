import { describe, expect, it } from 'vitest';

import type { LabelRule } from '#lib/ipc/index.ts';

import {
	MAX_FIRST_NUMBER,
	MAX_LABEL_CHARS,
	assignLabels,
	defaultRule,
	editableRules,
	formatNumber,
	labelsForPages,
	numberingProblem,
	pagesText,
	previewText,
	ruleFrom,
	runsOf,
	sameRules,
	type Numbering
} from './rules.ts';

const rule = (startPage: number, style: LabelRule['style'], prefix = '', firstNumber = 1): LabelRule => ({
	startPage,
	style,
	prefix,
	firstNumber
});

// The same expectations as the Rust tests in crates/pdf-core/src/labels.rs: the preview
// must show what Rust writes.
describe('formatting', () => {
	it('writes roman numerals', () => {
		expect([1, 2, 3, 4, 9, 12].map((n) => formatNumber(n, 'lowerRoman'))).toEqual([
			'i',
			'ii',
			'iii',
			'iv',
			'ix',
			'xii'
		]);
		expect(formatNumber(1994, 'upperRoman')).toBe('MCMXCIV');
		expect(formatNumber(4000, 'upperRoman')).toBe('MMMM');
	});

	it('repeats the letter past z', () => {
		expect([1, 26, 27, 28, 52].map((n) => formatNumber(n, 'lowerLetters'))).toEqual(['a', 'z', 'aa', 'bb', 'zz']);
		expect(formatNumber(53, 'upperLetters')).toBe('AAA');
	});

	it('keeps huge numbers and prefixes short', () => {
		expect(formatNumber(244_888, 'upperRoman')).toHaveLength(256);
		expect(formatNumber(245_000, 'upperRoman')).toBe('245000');
		expect(formatNumber(26 * 256, 'lowerLetters')).toBe('z'.repeat(256));
		expect(formatNumber(26 * 256 + 1, 'lowerLetters')).toBe('6657');
		const [label] = labelsForPages([rule(0, 'decimal', 'é'.repeat(10_000))], 1);
		expect([...label!]).toHaveLength(MAX_LABEL_CHARS);
	});

	it('applies rules with prefixes and start numbers', () => {
		const rules = [rule(0, 'lowerRoman'), rule(4, 'decimal'), rule(10, 'decimal', 'A-', 3), rule(12, 'none', 'Cover')];
		expect(labelsForPages(rules, 13)).toEqual([
			'i',
			'ii',
			'iii',
			'iv',
			'1',
			'2',
			'3',
			'4',
			'5',
			'6',
			'A-3',
			'A-4',
			'Cover'
		]);
	});

	it('numbers pages before the first rule in decimal (damaged files)', () => {
		expect(labelsForPages([rule(2, 'upperRoman')], 4)).toEqual(['1', '2', 'I', 'II']);
	});
});

describe('editing', () => {
	it('shows stored rules sorted, within the document, with a first-page rule', () => {
		const stored = [rule(9, 'decimal'), rule(3, 'lowerRoman'), rule(3, 'decimal'), rule(20, 'decimal')];
		const { rules, hidden } = editableRules(stored, 10);
		expect(rules).toEqual([defaultRule(), rule(3, 'lowerRoman'), rule(9, 'decimal')]);
		expect(hidden).toBe(1);
		expect(editableRules([], 5).rules).toEqual([defaultRule()]);
	});

	it('compares rules', () => {
		const rules = [defaultRule(), rule(4, 'decimal'), rule(8, 'upperLetters')];
		expect(sameRules(rules, [...rules])).toBe(true);
		expect(sameRules(rules, rules.slice(0, 2))).toBe(false);
		expect(sameRules(rules, [defaultRule(), rule(4, 'decimal', 'A-'), rule(8, 'upperLetters')])).toBe(false);
	});

	it('knows what a page has', () => {
		const rules = [rule(0, 'lowerRoman'), rule(4, 'decimal', 'A-', 3)];
		expect(ruleFrom(rules, 2)).toEqual(rule(2, 'lowerRoman', '', 3));
		expect(ruleFrom(rules, 6)).toEqual(rule(6, 'decimal', 'A-', 5));
		// Before the first rule of a damaged file, pages have their numbers.
		expect(ruleFrom([rule(2, 'upperRoman')], 1)).toEqual(rule(1, 'decimal', '', 2));
	});
});

describe('numbering the pages picked in the dialog', () => {
	const fresh = (style: LabelRule['style'], prefix = '', firstNumber = 1): Numbering => ({
		kind: 'new',
		style,
		prefix,
		firstNumber
	});
	const range = (from: number, to: number) => Array.from({ length: to - from + 1 }, (_, i) => from + i);
	const plain = [defaultRule()];
	const labels = (rules: LabelRule[], count = 8) => labelsForPages(rules, count);

	it('numbers a run of pages, and the pages after it keep their labels', () => {
		const rules = assignLabels(plain, 8, range(0, 2), fresh('lowerRoman'));
		expect(rules).toEqual([rule(0, 'lowerRoman'), rule(3, 'decimal', '', 4)]);
		expect(labels(rules)).toEqual(['i', 'ii', 'iii', '4', '5', '6', '7', '8']);
		// In the middle: the pages before keep theirs too.
		expect(labels(assignLabels(plain, 8, range(3, 4), fresh('upperLetters', 'App-')))).toEqual([
			'1',
			'2',
			'3',
			'App-A',
			'App-B',
			'6',
			'7',
			'8'
		]);
		// To the last page: nothing follows.
		expect(assignLabels(plain, 8, range(6, 7), fresh('decimal', '', 10))).toEqual([defaultRule(), rule(6, 'decimal', '', 10)]);
	});

	it('numbers through pages that are apart, leaving the ones between alone', () => {
		const rules = assignLabels(plain, 8, [1, 2, 6], fresh('lowerRoman'));
		expect(labels(rules)).toEqual(['1', 'i', 'ii', '4', '5', '6', 'iii', '8']);
		expect(rules.map((r) => r.startPage)).toEqual([0, 1, 3, 6, 7]);
	});

	it('replaces the rules inside the pages, and the page after still has its label', () => {
		const stored = [rule(0, 'lowerRoman'), rule(2, 'decimal'), rule(5, 'upperLetters')];
		// Pages 2 to 6 swallow the rule at page 3 and the one at page 6.
		const rules = assignLabels(stored, 8, range(1, 5), fresh('decimal', 'X', 1));
		expect(rules).toEqual([rule(0, 'lowerRoman'), rule(1, 'decimal', 'X', 1), rule(6, 'upperLetters', '', 2)]);
		expect(labels(rules)).toEqual(['i', 'X1', 'X2', 'X3', 'X4', 'X5', 'B', 'C']);
		// A rule that starts right after the pages stays as stored.
		expect(assignLabels(stored, 8, range(2, 4), fresh('none', 'Plate'))).toEqual([
			rule(0, 'lowerRoman'),
			rule(2, 'none', 'Plate'),
			rule(5, 'upperLetters')
		]);
	});

	it('carries on the numbering of the page before', () => {
		const stored = [rule(0, 'lowerRoman'), rule(3, 'decimal'), rule(6, 'upperLetters')];
		// Pages 4 and 5 join the roman pages; page 6 keeps "3".
		const rules = assignLabels(stored, 8, range(3, 4), { kind: 'continue' });
		expect(rules).toEqual([rule(0, 'lowerRoman'), rule(5, 'decimal', '', 3), rule(6, 'upperLetters')]);
		expect(labels(rules)).toEqual(['i', 'ii', 'iii', 'iv', 'v', '3', 'A', 'B']);
		// A whole range carried on is a range taken away.
		expect(assignLabels(stored, 8, range(3, 5), { kind: 'continue' })).toEqual([rule(0, 'lowerRoman'), rule(6, 'upperLetters')]);
		// The first page has no page before it: 1, 2, 3.
		expect(labels(assignLabels(stored, 8, range(0, 1), { kind: 'continue' }))).toEqual(['1', '2', 'iii', '1', '2', '3', 'A', 'B']);
	});

	it('adds no rule that only repeats the one before it', () => {
		// Every page numbered 1, 2, 3 is the plain document again.
		expect(assignLabels([rule(0, 'lowerRoman'), rule(3, 'decimal')], 8, range(0, 7), fresh('decimal'))).toEqual(plain);
		// Pages given the numbers they have: nothing changes.
		expect(assignLabels(plain, 8, range(2, 4), fresh('decimal', '', 3))).toEqual(plain);
		// A stored rule that repeats the one before it is not this change's to remove.
		const stored = [defaultRule(), rule(4, 'decimal', '', 5)];
		expect(assignLabels(stored, 8, [0], fresh('upperRoman'))).toEqual([
			rule(0, 'upperRoman'),
			rule(1, 'decimal', '', 2),
			rule(4, 'decimal', '', 5)
		]);
	});

	it('finds problems with a first number', () => {
		expect(numberingProblem(fresh('decimal', '', 1), 5)).toBeNull();
		expect(numberingProblem(fresh('decimal', '', 0), 5)).toMatch(/1 or higher/);
		expect(numberingProblem(fresh('decimal', '', Number.NaN), 5)).toMatch(/1 or higher/);
		expect(numberingProblem(fresh('decimal', '', 2.5), 5)).not.toBeNull();
		expect(numberingProblem(fresh('decimal', '', MAX_FIRST_NUMBER), 2)).toMatch(/too large/);
		expect(numberingProblem(fresh('decimal', '', MAX_FIRST_NUMBER), 1)).toBeNull();
		// A style without numbers has none to be wrong, nor has carrying on.
		expect(numberingProblem(fresh('none', 'Cover', Number.NaN), 5)).toBeNull();
		expect(numberingProblem({ kind: 'continue' }, 5)).toBeNull();
	});

	it('names the pages and previews their labels', () => {
		expect(runsOf([1, 2, 6])).toEqual([
			{ start: 1, end: 3 },
			{ start: 6, end: 7 }
		]);
		expect(pagesText([1, 2, 6])).toBe('2–3, 7');
		expect(pagesText([4])).toBe('5');
		expect(pagesText([0, 2, 4, 6, 8])).toBe('1, 3, 5, …');
		const shown = ['i', 'ii', 'iii', 'iv', 'v', '', '7'];
		expect(previewText(shown, [0, 1, 2])).toBe('i, ii, iii');
		expect(previewText(shown, [0, 1, 2, 3, 4, 6])).toBe('i, ii, iii … 7');
		expect(previewText(shown, [5])).toBe('(no label)');
	});
});
