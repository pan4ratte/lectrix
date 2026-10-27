// Compare (section 6.13): the arithmetic and wording of the Compare view, apart from its
// state so it can be tested by itself: where one document's place falls in the other, the
// marks each page shows, the groups of the list, and what the list and the progress say.

import type {
	CompareAnchor,
	CompareProgress,
	Difference,
	DifferenceGroup,
	DifferenceKind,
	DifferenceSide
} from '#lib/ipc/index.ts';

/** One of the two documents compared: the first (old) or the second (new). */
export type Side = 'old' | 'new';

export const otherSide = (side: Side): Side => (side === 'old' ? 'new' : 'old');

/**
 * Where `place` in one document (pages from the start, the part of a page as a fraction)
 * falls in the other, by the places the comparison lined them up at: between two of them,
 * as far as it is between them in this one; before the first and after the last, from the
 * documents' starts and ends. Without any (pages without text), the same place. Stays
 * within the other's `toCount` pages.
 */
export function mapPlace(
	anchors: readonly CompareAnchor[],
	place: number,
	from: Side,
	fromCount: number,
	toCount: number
): number {
	const clamp = (v: number) => Math.min(Math.max(v, 0), Math.max(0, toCount));
	if (anchors.length === 0) return clamp(place);
	const points: [number, number][] = [[0, 0]];
	for (const a of anchors) points.push(from === 'old' ? [a.old, a.new] : [a.new, a.old]);
	points.push([fromCount, toCount]);
	// Each point past the one before it in both, so the line between them goes forward.
	const line: [number, number][] = [];
	for (const p of points) {
		const last = line[line.length - 1];
		if (!last || (p[0] > last[0] && p[1] >= last[1])) line.push(p);
	}
	for (let i = 1; i < line.length; i++) {
		const [x0, y0] = line[i - 1]!;
		const [x1, y1] = line[i]!;
		if (place <= x1) return clamp(y0 + ((place - x0) / (x1 - x0)) * (y1 - y0));
	}
	const [xl, yl] = line[line.length - 1]!;
	return clamp(yl + (place - xl));
}

/** A difference as one page shows it: its text's rectangles, or where the other document's
 * text would go. */
export interface CompareMark {
	/** The difference's number in the comparison. */
	id: number;
	kind: DifferenceKind;
	/** `[x0, y0, x1, y1]`, page points (view space at zoom 1). */
	rect: [number, number, number, number];
	/** A caret where the other document's text would go, rather than text of this one. */
	caret: boolean;
}

/** How wide a caret mark is, in page points. */
const CARET_WIDTH = 2;

/** The marks each page of document `side` shows, for the differences not in `hidden`. */
export function marksByPage(
	differences: readonly Difference[],
	side: Side,
	hidden: ReadonlySet<number>
): Map<number, CompareMark[]> {
	const out = new Map<number, CompareMark[]>();
	const add = (page: number, mark: CompareMark) => {
		const list = out.get(page);
		if (list) list.push(mark);
		else out.set(page, [mark]);
	};
	differences.forEach((d, id) => {
		if (hidden.has(id)) return;
		const s = d[side];
		for (const r of s.rects) {
			add(r.page, { id, kind: d.kind, rect: [r.rect[0], r.rect[1], r.rect[2], r.rect[3]], caret: false });
		}
		if (s.rects.length === 0 && s.caret) {
			const c = s.caret;
			add(c.page, {
				id,
				kind: d.kind,
				rect: [c.x - CARET_WIDTH / 2, c.y0, c.x + CARET_WIDTH / 2, c.y1],
				caret: true
			});
		}
	});
	return out;
}

/** The difference whose mark is at a point of a page (page points), the smallest if several
 * are; null when none is. A caret is easier to hit than it is wide. */
export function markAt(marks: readonly CompareMark[], x: number, y: number, slop = 3): number | null {
	let best: { id: number; area: number } | null = null;
	for (const m of marks) {
		const pad = m.caret ? slop : 0;
		const [x0, y0, x1, y1] = m.rect;
		if (x < x0 - pad || x > x1 + pad || y < y0 || y > y1) continue;
		const area = (x1 - x0) * (y1 - y0);
		if (!best || area < best.area) best = { id: m.id, area };
	}
	return best?.id ?? null;
}

/** Where a difference is in document `side`: a rectangle to bring into view. */
export function placeIn(d: Difference, side: Side): { page: number; rect: [number, number, number, number] } {
	const s = d[side];
	const first = s.rects[0];
	if (first) {
		// The rectangles on the first page together.
		const on = s.rects.filter((r) => r.page === first.page).map((r) => r.rect);
		const rect: [number, number, number, number] = [
			Math.min(...on.map((r) => r[0])),
			Math.min(...on.map((r) => r[1])),
			Math.max(...on.map((r) => r[2])),
			Math.max(...on.map((r) => r[3]))
		];
		return { page: first.page, rect };
	}
	const c = s.caret;
	if (c) return { page: c.page, rect: [c.x, c.y0, c.x + 1, c.y1] };
	return { page: s.page, rect: [0, 0, 1, 1] };
}

export const GROUP_TITLES: Record<DifferenceGroup, string> = {
	body: 'Body text',
	margins: 'Headers and footers',
	numbering: 'Numbering'
};

export const KIND_NAMES: Record<DifferenceKind, string> = {
	inserted: 'Inserted',
	deleted: 'Deleted',
	changed: 'Changed'
};

/** The list's sections: the differences shown (not ignored, or all with `showIgnored`), in
 * FineReader's groups when `grouped`, else in one; empty groups left out. Each holds the
 * differences' numbers in the comparison. */
export function sections(
	differences: readonly Difference[],
	grouped: boolean,
	ignored: ReadonlySet<number>,
	showIgnored: boolean
): { group: DifferenceGroup | null; title: string; ids: number[] }[] {
	const shown = differences.map((_, i) => i).filter((i) => showIgnored || !ignored.has(i));
	if (!grouped) return shown.length ? [{ group: null, title: 'Differences', ids: shown }] : [];
	const order: DifferenceGroup[] = ['body', 'margins', 'numbering'];
	return order
		.map((group) => ({ group, title: GROUP_TITLES[group], ids: shown.filter((i) => differences[i]!.group === group) }))
		.filter((s) => s.ids.length > 0);
}

/** The differences the same as `id`: the same kind, with the same text in both documents
 * ("Ignore identical differences"). */
export function identical(differences: readonly Difference[], id: number): number[] {
	const d = differences[id];
	if (!d) return [];
	const same = (a: DifferenceSide, b: DifferenceSide) => a.text === b.text && a.rects.length > 0 === b.rects.length > 0;
	return differences
		.map((x, i) => ({ x, i }))
		.filter(({ x }) => x.kind === d.kind && same(x.old, d.old) && same(x.new, d.new))
		.map(({ i }) => i);
}

/** A page as the list names it: its label, else its number. */
export function pageName(page: number, labels: readonly string[] | null): string {
	const label = labels?.[page];
	return label ? label : String(page + 1);
}

/** A side's text as the list shows it: the text, "…" when cut short; a page area compared
 * as an image has none. */
export function sideText(s: DifferenceSide): string {
	if (!s.text) return s.rects.length ? '(an area of the page)' : '';
	return s.cut ? `${s.text}…` : s.text;
}

/** What "Copy difference" puts on the clipboard. */
export function copyText(
	d: Difference,
	names: { old: string; new: string },
	labels: { old: readonly string[] | null; new: readonly string[] | null }
): string {
	const line = (side: Side) => {
		const s = d[side];
		const text = sideText(s) || '—';
		return `${names[side]}, page ${pageName(s.page, labels[side])}: ${text}`;
	};
	return `${KIND_NAMES[d.kind]}\n${line('old')}\n${line('new')}`;
}

/** What the progress bar says, and how far along it is (0 to 1; null while it can't say). */
export function progressText(p: CompareProgress | null): { text: string; share: number | null } {
	if (!p) return { text: 'Starting…', share: null };
	switch (p.stage) {
		case 'reading':
			return {
				text: `Reading pages… ${p.done.toLocaleString()} of ${p.total.toLocaleString()}`,
				share: p.total ? p.done / p.total : null
			};
		case 'comparing':
			return { text: 'Comparing the text…', share: null };
		case 'images':
			return {
				text: `Comparing pages as images… ${p.done.toLocaleString()} of ${p.total.toLocaleString()}`,
				share: p.total ? p.done / p.total : null
			};
	}
}

/** "47 differences", "No differences". */
export function countText(n: number): string {
	if (n === 0) return 'No differences';
	return n === 1 ? '1 difference' : `${n.toLocaleString()} differences`;
}
