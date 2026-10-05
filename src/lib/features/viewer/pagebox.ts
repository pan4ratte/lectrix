// The page box: shows the current page and accepts a page number or a page label.

/** "iv (4 of 312)" with labels, "4 of 312" without (view bar, section 8). */
export function pagePosition(index: number, count: number, labels: readonly string[] | null) {
	const physical = `${index + 1} of ${count}`;
	const label = labels?.[index];
	if (label !== undefined && label !== '' && label !== String(index + 1)) {
		return `${label} (${physical})`;
	}
	return physical;
}

/**
 * What the view bar shows after the page box, as Acrobat does: "(4 of 312)" when the box
 * shows a label other than the page number, "of 312" otherwise.
 */
export function pageOf(index: number, count: number, labels: readonly string[] | null) {
	const label = labels?.[index];
	if (label !== undefined && label !== '' && label !== String(index + 1)) {
		return `(${index + 1} of ${count})`;
	}
	return `of ${count}`;
}

/** What the page box shows while not being edited: the label, or the page number. */
export function pageBoxText(index: number, labels: readonly string[] | null): string {
	const label = labels?.[index];
	return label !== undefined && label !== '' ? label : String(index + 1);
}

/**
 * The page index the user means by `input`: an exact page label first ("iv", "A-3"),
 * then a label ignoring case, then a physical page number. Null if nothing matches.
 */
export function resolvePageInput(
	input: string,
	labels: readonly string[] | null,
	pageCount: number
): number | null {
	const text = input.trim();
	if (!text) return null;
	if (labels) {
		const exact = labels.indexOf(text);
		if (exact >= 0) return exact;
		const lower = text.toLowerCase();
		const loose = labels.findIndex((l) => l.toLowerCase() === lower);
		if (loose >= 0) return loose;
	}
	if (/^\d+$/.test(text)) {
		const n = Number(text);
		if (n >= 1 && n <= pageCount) return n - 1;
	}
	return null;
}
