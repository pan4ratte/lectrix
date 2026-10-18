// The search bar's suggestions (section 6.1): the words of the document that contain what
// is typed, each with the number of matches a search for it finds, as Acrobat lists them.
//
// Rust sends the words around each match within a line, as the text spells them
// (`words_containing` in text.rs). They are gathered here case-insensitively, under the
// spelling the document uses most.

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

/** A word as a search compares it: lower case, and any run of spaces as one space. */
export function foldWord(text: string): string {
	return text.trim().replace(/\s+/g, ' ').toLocaleLowerCase();
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
	const at = typed ? text.toLocaleLowerCase().indexOf(typed) : -1;
	if (at < 0) return [text, '', ''];
	return [text.slice(0, at), text.slice(at, at + typed.length), text.slice(at + typed.length)];
}
