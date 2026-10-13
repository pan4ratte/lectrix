// Where a document's pages are after a change that moved, deleted or inserted some.
// Rust reports it with every such change, an undo or a redo of one included
// (`DocumentChange.pageMap`): for each page now, the index it had before, or null for a
// page that was not there. What the frontend keeps by page index follows its page with this.

import type { ViewPosition } from '#lib/features/viewer/history.ts';

export class PageMove {
	private readonly now = new Map<number, number>();

	constructor(private readonly map: readonly (number | null)[]) {
		map.forEach((before, index) => {
			if (before !== null) this.now.set(before, index);
		});
	}

	/** Where the page that was at `page` is now, or null if it is gone. */
	to(page: number): number | null {
		return this.now.get(page) ?? null;
	}

	/** Where that page is now; for one that is gone, the page now in its place (the last
	 * page, past the end). */
	follow(page: number): number {
		return this.to(page) ?? Math.max(0, Math.min(page, this.map.length - 1));
	}

	/** The same place on the page it was on; the top of the page in its place if it is gone. */
	position(position: ViewPosition): ViewPosition {
		const page = this.to(position.page);
		return page === null ? { page: this.follow(position.page), offset: 0 } : { page, offset: position.offset };
	}

	/** The pages' keys now: each page keeps its own, and a new page gets one from `fresh`. */
	keys(before: readonly number[], fresh: () => number): number[] {
		return this.map.map((old) => (old === null ? undefined : before[old]) ?? fresh());
	}
}
