// Synchronized scrolling (section 6.1, split view): the panes of a joined tab move together
// once the view bar's lock is on. The pane scrolled (by the user, or by what the user asked
// of it: a page turned, a bookmark followed) leads; the others keep the distance they had
// from it when it took the lead. Panes that show the same document also take its zoom, its
// view rotation and its pages turned on screen (`sameDocument`, `zoomed`). Synchronized
// zoom, a lock of its own, has every pane take the zoom of the pane zoomed (`zoomed`).

import type { ZoomMode } from '#lib/features/viewer/zoom.ts';
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
	// Panes of the same document take the zoom of the one in focus.
	group.zooms.clear();
}

/** The view bar's lock, for the tab shown (or `group`). */
export function toggleLock(group = app.activeGroup) {
	if (group?.joined) setLocked(group, !group.locked);
}

/** Turns synchronized zoom of a joined tab on or off: on, every pane takes the zoom of the
 * one in focus. */
export function setZoomLocked(group: TabGroup, on: boolean) {
	group.zoomLocked = on && group.joined;
	group.zooms.clear();
}

/** Synchronized zoom, for the tab shown (or `group`). */
export function toggleZoomLock(group = app.activeGroup) {
	if (group?.joined) setZoomLocked(group, !group.zoomLocked);
}

/**
 * A pane scrolled, not by following the others: if its tab is locked, it leads, and the
 * other panes go as far from where they were as it went. A pane that takes the lead from
 * another starts from where everything is now.
 */
export function scrolled(view: DocTab) {
	// A pane of the Compare view follows the comparison's own lines (section 6.13).
	if (view.comparing) {
		view.comparing.scrolled();
		return;
	}
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

/** What panes of the same document as the lead take from it, besides its place and zoom:
 * view rotation and pages turned on screen. Run by the split view while the lock is on. */
export function sameDocument(group: TabGroup) {
	const lead = group.lead ?? group.focused;
	if (!group.locked || !lead) return;
	for (const view of group.views) {
		if (view === lead || view.doc !== lead.doc) continue;
		if (view.rotation !== lead.rotation) view.rotation = lead.rotation;
		if (view.pageTurns !== lead.pageTurns) view.pageTurns = lead.pageTurns;
	}
}

/** Whether `view` takes the zoom of `source`: with synchronized zoom, every pane does; with
 * synchronized scrolling alone, the panes of the same document. */
function followsZoom(group: TabGroup, source: DocTab, view: DocTab): boolean {
	return group.zoomLocked || (group.locked && view.doc === source.doc);
}

/** A pane's zoom as `zooms` keeps it: a fit (whatever zoom that makes in the pane), or the
 * zoom itself. */
export function zoomKey(mode: ZoomMode, zoom: number): string {
	return mode === 'custom' ? `custom:${zoom.toFixed(4)}` : mode;
}

/**
 * Some pane's zoom changed, or panes began to follow zooms: the pane zoomed (the one in
 * focus, when several were, or when none was seen before) leads, and the panes that follow
 * it take its zoom, at once, a fit as a fit of their own. Run by the split view as any
 * pane's zoom changes, while either lock is on.
 */
export function zoomed(group: TabGroup) {
	if (!group.locked && !group.zoomLocked) return;
	const views = group.views.filter((v) => v.viewer);
	const changed = views.filter((v) => group.zooms.get(v) !== zoomKey(v.zoomMode, v.zoom));
	if (changed.length === 0) return;
	const seen = changed.filter((v) => group.zooms.has(v));
	const focused = group.focused;
	const source =
		focused && (seen.includes(focused) || (seen.length === 0 && views.includes(focused))) ? focused : (seen[0] ?? null);
	if (!source) return;
	const key = zoomKey(source.zoomMode, source.zoom);
	for (const view of views) {
		if (view === source || !followsZoom(group, source, view)) {
			group.zooms.set(view, zoomKey(view.zoomMode, view.zoom));
			continue;
		}
		group.zooms.set(view, key);
		if (zoomKey(view.zoomMode, view.zoom) === key) continue;
		void followZoom(group, view, source.zoom, source.zoomMode);
	}
}

async function followZoom(group: TabGroup, view: DocTab, zoom: number, mode: ZoomMode) {
	await view.viewer?.followZoom(zoom, mode);
	// Zooming kept the top of the view where it was; while the panes scroll together, the
	// lead may have moved it since (its own zoom moves it).
	const place = group.locked ? group.places.get(view) : undefined;
	if (place !== undefined && group.lead !== view) view.viewer?.follow(place);
}
