// Page ranges typed by the user ("1-3, 7"), physical page numbers starting at 1.

/** Parses "1-3, 7" into sorted, unique 0-based page indexes. Null if invalid. */
export function parsePageRanges(text: string, pageCount: number): number[] | null {
	const pages = new Set<number>();
	const parts = text.split(/[,;]/).map((p) => p.trim()).filter(Boolean);
	if (parts.length === 0) return null;
	for (const part of parts) {
		const m = /^(\d+)\s*(?:[-–]\s*(\d+))?$/.exec(part);
		if (!m) return null;
		const from = Number(m[1]);
		const to = m[2] === undefined ? from : Number(m[2]);
		if (from < 1 || to < from || to > pageCount) return null;
		for (let p = from; p <= to; p++) pages.add(p - 1);
	}
	return [...pages].sort((a, b) => a - b);
}
