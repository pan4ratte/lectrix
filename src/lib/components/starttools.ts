// The start screen's tool cards (StartScreen.svelte): how they share the grid's rows.

/**
 * How many of the grid's six tracks each of `count` cards spans, the grid showing `columns`
 * cards to a row (2 or 3): three tracks for two to a row, two for three; the cards of a last
 * row that is not full share it, one across the whole row, two half each.
 */
export function toolSpans(count: number, columns: 2 | 3 = 3): number[] {
	const rest = count % columns;
	const full = count - rest;
	return Array.from({ length: count }, (_, i) => (i < full ? 6 / columns : 6 / rest));
}
