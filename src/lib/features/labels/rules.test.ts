import { describe, expect, it } from 'vitest';

import type { LabelRule } from '#lib/ipc/index.ts';

import {
	MAX_LABEL_CHARS,
	defaultRule,
	editableRules,
	formatNumber,
	labelsForPages,
	pagesText,
	rangeEnds,
	rangeText,
	replaceRule,
	romanThenArabic,
	ruleProblem,
	sameRules,
	withRuleAt
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

	it('finds problems with a rule', () => {
		const rules = [defaultRule(), rule(4, 'decimal')];
		expect(ruleProblem(rule(0, 'none', 'x'), rules, 0, 10)).toBeNull();
		expect(ruleProblem(rule(1, 'decimal'), rules, 0, 10)).toMatch(/first rule/);
		expect(ruleProblem(rule(0, 'decimal'), rules, 1, 10)).toMatch(/Page 1 already/);
		expect(ruleProblem(rule(10, 'decimal'), rules, 1, 10)).toMatch(/between 1 and 10/);
		expect(ruleProblem(rule(4, 'decimal'), [...rules, rule(7, 'decimal')], 2, 10)).toMatch(/page 5/);
		expect(ruleProblem(rule(4, 'decimal', '', 0), rules, 1, 10)).toMatch(/1 or higher/);
		expect(ruleProblem(rule(4, 'decimal', '', 2 ** 31), rules, 1, 10)).toMatch(/too large/);
		expect(ruleProblem(rule(4.5, 'decimal'), rules, 1, 10)).not.toBeNull();
	});

	it('replaces and adds rules in page order', () => {
		const rules = [defaultRule(), rule(4, 'decimal'), rule(8, 'upperLetters')];
		expect(replaceRule(rules, 2, rule(2, 'upperLetters')).map((r) => r.startPage)).toEqual([0, 2, 4]);
		const added = withRuleAt(rules, 6);
		expect(added.map((r) => r.startPage)).toEqual([0, 4, 6, 8]);
		expect(added[2]).toEqual(defaultRule(6));
		expect(withRuleAt(rules, 4)).toEqual(rules);
		expect(sameRules(rules, [...rules])).toBe(true);
		expect(sameRules(rules, added)).toBe(false);
	});

	it('builds the roman front matter preset', () => {
		expect(labelsForPages(romanThenArabic(3), 6)).toEqual(['i', 'ii', 'iii', '1', '2', '3']);
	});

	it('describes ranges', () => {
		const labels = ['i', 'ii', 'iii', '1', ''];
		const ends = rangeEnds([defaultRule(), rule(3, 'decimal'), rule(4, 'none')], 5);
		expect(ends).toEqual([3, 4, 5]);
		expect(rangeText(labels, 0, 3)).toBe('i – iii');
		expect(rangeText(labels, 3, 4)).toBe('1');
		expect(rangeText(labels, 4, 5)).toBe('(no label)');
		expect(pagesText(0, 3)).toBe('1–3');
		expect(pagesText(3, 4)).toBe('4');
	});
});
