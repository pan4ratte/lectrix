import { describe, expect, it } from 'vitest';

import { FoundWords, MAX_SUGGESTIONS, foldWord, splitMatch } from './suggestions.ts';

describe('search suggestions', () => {
	it('gathers spellings of a word under the one used most', () => {
		const words = new FoundWords();
		words.add([
			{ text: 'Lecture', count: 1 },
			{ text: 'lecture', count: 3 }
		]);
		words.add([{ text: 'LECTURE', count: 1 }]);
		expect(words.suggest('lect')).toEqual([{ text: 'lecture', count: 5 }]);
	});

	it('counts what a search for the word finds, longer words included', () => {
		const words = new FoundWords();
		words.add([
			{ text: 'lecture', count: 2 },
			{ text: 'lectures', count: 3 },
			{ text: 'lecturer', count: 1 },
			{ text: 'elect', count: 4 }
		]);
		expect(words.suggest('lect')).toEqual([
			{ text: 'lecture', count: 6 },
			{ text: 'elect', count: 4 },
			{ text: 'lectures', count: 3 },
			{ text: 'lecturer', count: 1 }
		]);
	});

	it('leaves out the word typed, and keeps to the most common', () => {
		const words = new FoundWords();
		words.add([{ text: 'the', count: 50 }, ...Array.from({ length: 20 }, (_, i) => ({ text: `the${'x'.repeat(i + 1)}`, count: 1 }))]);
		const shown = words.suggest('The ');
		expect(shown).toHaveLength(MAX_SUGGESTIONS);
		expect(shown.map((s) => s.text)).not.toContain('the');
		words.clear();
		expect(words.suggest('the')).toEqual([]);
	});

	it('finds the match in a suggestion', () => {
		expect(splitMatch('Lectures', 'ECT')).toEqual(['L', 'ect', 'ures']);
		expect(splitMatch('on  lecture', 'n lec')).toEqual(['on  lecture', '', '']);
		expect(foldWord('  On   LEC ')).toBe('on lec');
	});
});
