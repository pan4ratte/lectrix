// Synchronized scrolling (section 6.1, split view): the panes of a joined tab move together
// once the view bar's lock is on. The pane scrolled (by the user, or by what the user asked
// of it: a page turned, a bookmark followed) leads; the others keep the distance they had
// from it when it took the lead. Panes that show the same document also take its zoom, its
// view rotation and its pages turned on screen (`sameDocument`).

import { app } from '#lib/stores/app.svelte.ts';
import type { DocTab } from '#lib/stores/doc.svelte.ts';
import type { TabGroup } from '#lib/stores/group.svelte.ts';

import { followerPlace } from './split.ts';

function placeOf(view: DocTab): number {
	return view.viewer?.place() ?? view.currentPage;
}

/** Turns the lock of a joined tab on or off. */
export function setLocked(group: TabGroup, on: boolean) {
	group.locked = on && group.joined;
	group.lead = group.locked ? group.focused : null;
	group.places = new Map(group.views.map((v) => [v, placeOf(v)]));
	group.anchors = new Map(group.places);
}

/** The view bar's lock, for the tab shown. */
export function toggleLock() {
	const group = app.activeGroup;
	if (group?.joined) setLocked(group, !group.locked);
}

/**
 * A pane scrolled, not by following the others: if its tab is locked, it leads, and the
 * other panes go as far from where they were as it went. A pane that takes the lead from
 * another starts from where everything is now.
 */
export function scrolled(view: DocTab) {
	const group = app.groupOf(view);
	if (!group?.locked || !view.viewer) return;
	const now = view.viewer.place();
	if (group.lead !== view) {
		group.lead = view;
		for (const v of group.views) if (!group.places.has(v)) group.places.set(v, placeOf(v));
		group.anchors = new Map(group.places);
	}
	group.places.set(view, now);
	const from = group.anchors.get(view) ?? now;
	for (const other of group.views) {
		if (other === view || !other.viewer) continue;
		let anchor = group.anchors.get(other);
		if (anchor === undefined) {
			anchor = placeOf(other) - (now - from);
			group.anchors.set(other, anchor);
		}
		const place = followerPlace(anchor, from, now, other.pageCount);
		group.places.set(other, place);
		other.viewer.follow(place);
	}
}

/** What panes of the same document as the lead take from it, besides its place: zoom, view
 * rotation and pages turned on screen. Run by the split view while the lock is on. */
export function sameDocument(group: TabGroup) {
	const lead = group.lead ?? group.focused;
	if (!group.locked || !lead) return;
	for (const view of group.views) {
		if (view === lead || view.doc !== lead.doc) continue;
		if (view.rotation !== lead.rotation) view.rotation = lead.rotation;
		if (view.pageTurns !== lead.pageTurns) view.pageTurns = lead.pageTurns;
		if (lead.zoomMode !== 'custom') {
			if (view.zoomMode !== lead.zoomMode) view.viewer?.fit(lead.zoomMode);
		} else if (view.zoomMode !== 'custom' || Math.abs(view.zoom - lead.zoom) > 1e-3) {
			view.viewer?.setZoom(lead.zoom, 'custom');
		}
	}
}
