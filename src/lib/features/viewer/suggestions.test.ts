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
		// Spaces match as the search matches them: any run as one.
		expect(splitMatch('on  lecture', 'n lec')).toEqual(['o', 'n  lec', 'ture']);
		expect(splitMatch('Lectures', 'xyz')).toEqual(['Lectures', '', '']);
		expect(foldWord('  On   LEC ')).toBe('on lec');
	});

	it('compares words as the search does, without accents', () => {
		expect(foldWord('Łódź')).toBe('lodz');
		expect(foldWord('Straße STRAẞE')).toBe('strasse strasse');
		expect(foldWord('Øresund Ærø œuvre Þórshöfn')).toBe('oresund aero oeuvre thorshofn');
		expect(foldWord('Đakovo Ħamrun Iğdır İstanbul')).toBe('dakovo hamrun igdir istanbul');
		expect(foldWord('Tiếng Việt ΆΘΗΝΑ ὁδός')).toBe('tieng viet αθηνα οδοσ');
		expect(foldWord('ﬁnd ＡＢＣ x²')).toBe('find abc x2');
		expect(foldWord('cafe\u{301}')).toBe('cafe');
		// Ё is Е, but Й, Ї and Ў are letters of their own, however they are written.
		expect(foldWord('Ёлка Мой мои\u{306} Її Ўў')).toBe('елка мой мой її ўў');
		// Other scripts keep their marks; Korean syllables stay whole.
		expect(foldWord('กิน हिन्दी')).toBe('กิน हिन्दी');
		expect(foldWord('が')).toBe(foldWord('か\u{3099}'));
		expect(foldWord('\u{1112}\u{1161}\u{11ab}')).toBe('한');
	});

	it('compares Arabic and Hebrew without their points, and quotes as straight', () => {
		expect(foldWord('שָׁלוֹם \u{fb2a}לום')).toBe('שלום שלום');
		expect(foldWord('مُحَمَّد كـــتـــاب أحمد إسلام آمن')).toBe('محمد كتاب احمد اسلام امن');
		expect(foldWord('\u{fee3}\u{fea4}\u{fee4}\u{feaa}')).toBe('محمد');
		expect(foldWord('Don’t “Hello” 3″ пʼять')).toBe('don\'t "hello" 3" п\'ять');
		expect(splitMatch('שָׁלוֹם', 'של')).toEqual(['', 'שָׁל', 'וֹם']);
		expect(splitMatch('Don’t', "n't")).toEqual(['Do', 'n’t', '']);
	});

	it('gathers words that differ in their accents, and counts them together', () => {
		const words = new FoundWords();
		words.add([
			{ text: 'Łódź', count: 3 },
			{ text: 'Lodz', count: 1 },
			{ text: 'Straße', count: 2 },
			{ text: 'Strasse', count: 1 }
		]);
		expect(words.suggest('o')).toEqual([
			{ text: 'Łódź', count: 4 },
			{ text: 'Straße', count: 3 }
		]);
	});

	it('marks the whole letters a match covers in a suggestion', () => {
		expect(splitMatch('Łódź', 'lod')).toEqual(['', 'Łód', 'ź']);
		expect(splitMatch('Straße', 'ss')).toEqual(['Stra', 'ß', 'e']);
		expect(splitMatch('Straße', 'asse')).toEqual(['Str', 'aße', '']);
		expect(splitMatch('cafe\u{301}s', 'cafe')).toEqual(['', 'café', 's']);
		expect(splitMatch('ёжик', 'еж')).toEqual(['', 'ёж', 'ик']);
	});
});
