// Documents side by side in one window (section 6.1, split view): the arithmetic of the
// panes of a joined tab, kept apart from the stores so it can be tested by itself.

/** The most panes a joined tab has. */
export const MAX_PANES = 4;

/** The narrowest a pane gets while its divider is dragged, CSS pixels. */
export const MIN_PANE_PX = 240;

/** `n` panes of the same width, as shares of the whole. */
export function equalWidths(n: number): number[] {
	return Array.from({ length: n }, () => 1 / n);
}

/**
 * The shares after the divider between pane `i` and pane `i + 1` moves by `dx` pixels, of
 * `total` pixels for all the panes: only those two change, each kept at least `min` pixels
 * wide (or as wide as it was, if it was narrower already).
 */
export function resizeWidths(widths: readonly number[], i: number, dx: number, total: number, min = MIN_PANE_PX): number[] {
	const out = [...widths];
	const a = widths[i];
	const b = widths[i + 1];
	if (a === undefined || b === undefined || total <= 0) return out;
	const pair = (a + b) * total;
	const minA = Math.min(min, a * total);
	const minB = Math.min(min, b * total);
	const left = Math.min(Math.max(a * total + dx, minA), pair - minB);
	out[i] = left / total;
	out[i + 1] = (pair - left) / total;
	return out;
}

/** The names the panes show: a document shown twice is "report.pdf (2)" the second time. */
export function paneNames(views: readonly { id: number; name: string }[]): string[] {
	const seen = new Map<number, number>();
	return views.map((v) => {
		const n = (seen.get(v.id) ?? 0) + 1;
		seen.set(v.id, n);
		return n === 1 ? v.name : `${v.name} (${n})`;
	});
}

/**
 * Where a pane locked to the others goes (section 6.1, synchronized scrolling): as far from
 * where it was when they were last lined up (`anchor`) as the pane being scrolled is from
 * its own (`driverAnchor`, `driverNow`), so page 3 beside page 10 stays page 3 beside page
 * 10. Positions are pages from the start, with the part of a page as a fraction; the result
 * stays within the pane's `pageCount` pages.
 */
export function followerPlace(anchor: number, driverAnchor: number, driverNow: number, pageCount: number): number {
	const place = anchor + (driverNow - driverAnchor);
	return Math.min(Math.max(place, 0), Math.max(0, pageCount));
}

/** How wide the bands at the edges of the pages are, where a tab dragged there joins the
 * one shown (as a share of their width, within these limits in CSS pixels). */
export const EDGE_SHARE = 0.2;
export const EDGE_MIN = 64;
export const EDGE_MAX = 200;

/**
 * Where a tab dropped `x` pixels from the left of pages `total` pixels wide would join the
 * tab shown, whose panes have `widths`: before pane `at` (0 at the left edge, the number of
 * panes at the right). Only near an edge or a divider: null over the middle of a pane, where
 * a dropped tab stays as it was (section 8).
 */
export function joinIndex(x: number, total: number, widths: readonly number[]): number | null {
	if (total <= 0 || x < 0 || x > total) return null;
	const band = Math.min(EDGE_MAX, Math.max(EDGE_MIN, total * EDGE_SHARE));
	if (x <= band) return 0;
	if (x >= total - band) return widths.length;
	let edge = 0;
	for (let i = 0; i < widths.length - 1; i++) {
		edge += (widths[i] ?? 0) * total;
		if (Math.abs(x - edge) <= band / 2) return i + 1;
	}
	return null;
}
