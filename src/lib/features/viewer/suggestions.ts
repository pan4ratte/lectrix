// The search bar's suggestions (section 6.1): the words of the document that contain what
// is typed, each with the number of matches a search for it finds, as Acrobat lists them.
//
// Rust sends the words around each match within a line, as the text spells them
// (`words_containing` in text.rs). They are gathered here as the search compares them
// (`foldWord`: without case or accents), under the spelling the document uses most.

import type { SearchWord } from '#lib/ipc/index.ts';

export interface Suggestion {
	/** The word as the document spells it most often. */
	text: string;
	/** How many matches a search for it finds: every word that contains it counts. */
	count: number;
}

/** The most suggestions shown. */
export const MAX_SUGGESTIONS = 8;
/** The most common words looked at for them: the counts are worked out for these alone. */
const CANDIDATES = 24;

/** The marks a search looks past (`is_ignored_mark` in fold.rs): the combining accents of
 * Latin, Greek and Cyrillic letters, Hebrew's points and cantillation, Arabic's vowel
 * marks, shadda, sukun, hamza, madda and Quranic marks. */
const ACCENTS = new RegExp(
	'[\\u{300}-\\u{36f}\\u{1ab0}-\\u{1aff}\\u{1dc0}-\\u{1dff}\\u{20d0}-\\u{20ff}\\u{fe20}-\\u{fe2f}' +
		'\\u{591}-\\u{5bd}\\u{5bf}\\u{5c1}-\\u{5c2}\\u{5c4}-\\u{5c5}\\u{5c7}' +
		'\\u{610}-\\u{61a}\\u{64b}-\\u{65f}\\u{670}\\u{6d6}-\\u{6dc}\\u{6df}-\\u{6e4}\\u{6e7}-\\u{6e8}' +
		'\\u{6ea}-\\u{6ed}\\u{8d3}-\\u{8e1}\\u{8e3}-\\u{8ff}]',
	'gu'
);
/** Apostrophes and quotes, curly or as primes, as the straight ones people type (`push` in
 * fold.rs). */
const QUOTES: Readonly<Record<string, string>> = {
	'\u{2018}': "'",
	'\u{2019}': "'",
	'\u{201a}': "'",
	'\u{201b}': "'",
	'\u{2032}': "'",
	'\u{2035}': "'",
	'\u{2bc}': "'",
	'\u{201c}': '"',
	'\u{201d}': '"',
	'\u{201e}': '"',
	'\u{201f}': '"',
	'\u{2033}': '"',
	'\u{2036}': '"'
};
/** Letters with no accent to take off, as the letters people type for them (`letter` in
 * fold.rs). */
const LETTERS: Readonly<Record<string, string>> = {
	ø: 'o',
	ł: 'l',
	đ: 'd',
	ð: 'd',
	ħ: 'h',
	ı: 'i',
	ŧ: 't',
	ς: 'σ',
	ß: 'ss',
	æ: 'ae',
	œ: 'oe',
	þ: 'th'
};
/** Letters of their own, not a letter with an accent: Й, Ї and Ў (Ё is Е). */
const KEPT = new Set(['й', 'Й', 'ї', 'Ї', 'ў', 'Ў']);

/** One character as a search compares it (`push` in fold.rs), put back together (so a
 * Korean syllable stays one): a space stays a space, for the caller to squash. */
function foldChar(c: string): string {
	if (/\s/u.test(c)) return ' ';
	// A soft hyphen, and Arabic's tatweel, which only stretches a word.
	if (c === '\u{ad}' || c === '\u{640}') return '';
	if (KEPT.has(c)) return c.toLowerCase();
	const quote = QUOTES[c];
	if (quote) return quote;
	let out = '';
	for (const part of c.normalize('NFKD').replace(ACCENTS, '').toLowerCase().replace(ACCENTS, '')) {
		out += /\s/u.test(part) ? ' ' : (LETTERS[part] ?? part);
	}
	return out.normalize('NFC');
}

/** Each character of `text` (put together first, so an accent written after its letter is
 * one character with it), folded. */
function foldedChars(text: string): { chars: string[]; folds: string[] } {
	const chars = [...text.normalize('NFC')];
	return { chars, folds: chars.map(foldChar) };
}

/**
 * A word as a search compares it, as Rust's search does (fold.rs): lower case, without
 * accents, Arabic and Hebrew points or compatibility forms, letters such as ł, ø and ß as
 * l, o and ss, curly quotes as straight ones, and any run of spaces as one space.
 */
export function foldWord(text: string): string {
	return foldedChars(text).folds.join('').replace(/\s+/g, ' ').trim();
}

/** The words found so far, gathered by `foldWord`. */
export class FoundWords {
	/** Folded word, then each spelling of it and how often the document has that. */
	private readonly words = new Map<string, Map<string, number>>();

	add(found: readonly SearchWord[]) {
		for (const { text, count } of found) {
			const key = foldWord(text);
			const forms = this.words.get(key) ?? new Map<string, number>();
			forms.set(text, (forms.get(text) ?? 0) + count);
			this.words.set(key, forms);
		}
	}

	clear() {
		this.words.clear();
	}

	/**
	 * The words to suggest for `query`, most matches first: not the query itself, which the
	 * bar counts already. A word's count is what a search for it would find, so it counts
	 * the longer words it is part of too ("lecture" counts "lectures").
	 */
	suggest(query: string): Suggestion[] {
		const typed = foldWord(query);
		const entries = [...this.words].map(([key, forms]) => {
			let total = 0;
			let text = '';
			let most = 0;
			for (const [form, count] of forms) {
				total += count;
				if (count > most) [text, most] = [form, count];
			}
			return { key, text, total };
		});
		const candidates = entries
			.filter((e) => e.key !== typed)
			.sort((a, b) => b.total - a.total || a.key.localeCompare(b.key))
			.slice(0, CANDIDATES);
		return candidates
			.map(({ key, text }) => {
				let count = 0;
				for (const e of entries) if (e.key.includes(key)) count += occurrences(e.key, key) * e.total;
				return { text, count };
			})
			.sort((a, b) => b.count - a.count || a.text.localeCompare(b.text))
			.slice(0, MAX_SUGGESTIONS);
	}
}

/** How many times `part` is in `text`, one after another as a search finds them. */
function occurrences(text: string, part: string): number {
	let n = 0;
	for (let at = text.indexOf(part); at >= 0; at = text.indexOf(part, at + part.length)) n++;
	return n;
}

/** `text` cut into the part before the first match of `query`, the match and the rest, to
 * show the match in a suggestion. */
export function splitMatch(text: string, query: string): [string, string, string] {
	const typed = foldWord(query);
	if (!typed) return [text, '', ''];
	// The folded text, a run of spaces as one, and where each character's fold starts in it.
	const { chars, folds } = foldedChars(text);
	let folded = '';
	const starts: number[] = [];
	for (const f of folds) {
		starts.push(folded.length);
		folded += f === ' ' && folded.endsWith(' ') ? '' : f;
	}
	starts.push(folded.length);
	const at = folded.indexOf(typed);
	if (at < 0) return [text.normalize('NFC'), '', ''];
	// The characters the match covers, whole, and an accent folded away after them.
	let first = 0;
	while ((starts[first + 1] ?? Infinity) <= at) first++;
	let end = first + 1;
	while ((starts[end] ?? Infinity) < at + typed.length) end++;
	while (end < chars.length && folds[end] === '') end++;
	return [chars.slice(0, first).join(''), chars.slice(first, end).join(''), chars.slice(end).join('')];
}
