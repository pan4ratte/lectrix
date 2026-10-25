// One tab of the window's row: a view of one document, or views joined side by side
// (section 6.1, split view), up to four panes. The row and which tab is shown are the app
// store's.

import { equalWidths, MAX_PANES, paneNames } from '#lib/features/split/split.ts';

import type { DocTab } from './doc.svelte.ts';

let groupSeq = 0;

export class TabGroup {
	readonly key = ++groupSeq;
	/** Its panes, left to right; one for an ordinary tab. */
	views = $state<DocTab[]>([]);
	/** Each pane's share of the width, together 1. */
	widths = $state<number[]>([]);
	/** The pane the view bar, the side panels and the commands act on. */
	focus = $state(0);
	/** The panes scroll together (section 6.1, synchronized scrolling). */
	locked = $state(false);
	/** While they do, the pane being scrolled, which the others follow. */
	lead = $state<DocTab | null>(null);
	/** Each pane's place when the lead last changed, and since (pages from the start, the
	 * part of a page as a fraction): `features/split/sync.ts`. */
	anchors = new Map<DocTab, number>();
	places = new Map<DocTab, number>();
	/** The panes take the zoom of the one zoomed (section 6.1, synchronized zoom). */
	zoomLocked = $state(false);
	/** Each pane's zoom as last seen while panes follow zooms, to tell which one was zoomed
	 * (`features/split/sync.ts`). */
	zooms = new Map<DocTab, string>();

	focused = $derived(this.views[Math.min(this.focus, this.views.length - 1)] ?? null);
	joined = $derived(this.views.length > 1);
	names = $derived(paneNames(this.views));
	full = $derived(this.views.length >= MAX_PANES);

	constructor(views: DocTab[]) {
		this.views = views;
		this.widths = equalWidths(views.length);
	}

	has(view: DocTab): boolean {
		return this.views.includes(view);
	}

	/** Its panes become `views`, of the same width again, with `focused` the one in focus
	 * (else the one that was, if it stays). A tab of one pane scrolls by itself. */
	setViews(views: DocTab[], focused: DocTab | null = this.focused) {
		this.views = views;
		this.widths = equalWidths(views.length);
		const at = focused ? views.indexOf(focused) : -1;
		this.focus = at >= 0 ? at : Math.min(this.focus, Math.max(0, views.length - 1));
		if (views.length < 2) {
			this.locked = false;
			this.zoomLocked = false;
		}
		if (this.lead && !views.includes(this.lead)) this.lead = null;
		for (const view of this.zooms.keys()) if (!views.includes(view)) this.zooms.delete(view);
	}

	/** Puts the focus on `view`, one of its panes. */
	focusOn(view: DocTab) {
		const at = this.views.indexOf(view);
		if (at >= 0) this.focus = at;
	}

	/** The name its pane shows. */
	nameOf(view: DocTab): string {
		return this.names[this.views.indexOf(view)] ?? view.name;
	}
}
