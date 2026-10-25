// The start screen's tool cards (StartScreen.svelte): how they share the grid's rows.

/**
 * How many of the grid's six tracks each of `count` cards spans: two, so three cards to a
 * row; the cards of a last row that is not full share it, one across the whole row, two
 * half each.
 */
export function toolSpans(count: number): number[] {
	const rest = count % 3;
	const full = count - rest;
	return Array.from({ length: count }, (_, i) => (i < full ? 2 : rest === 1 ? 6 : 3));
}
