import { describe, expect, it, vi } from 'vitest';

import type { ZoomMode } from '#lib/features/viewer/zoom.ts';
import type { DocTab } from '#lib/stores/doc.svelte.ts';

vi.mock('#lib/stores/app.svelte.ts', () => ({ app: {} }));

const { TabGroup } = await import('#lib/stores/group.svelte.ts');
const { setLocked, setZoomLocked, zoomed } = await import('./sync.ts');

/** A pane as synchronized zoom sees it: its document, its zoom, and a viewer that takes
 * another pane's zoom as the real one does (a fit makes a zoom of its own). */
function pane(doc: object, zoom: number, zoomMode: ZoomMode = 'custom') {
	const view = {
		doc,
		zoom,
		zoomMode,
		pageCount: 10,
		viewer: {
			place: () => 0,
			follow: vi.fn(),
			followZoom: vi.fn(async (z: number, mode: ZoomMode) => {
				view.zoomMode = mode;
				view.zoom = mode === 'custom' ? z : 0.77;
			})
		}
	};
	return view;
}

type Pane = ReturnType<typeof pane>;

function joined(...views: Pane[]) {
	return new TabGroup(views as unknown as DocTab[]);
}

describe('synchronized zoom', () => {
	it('gives every pane the zoom of the one in focus as it is turned on', () => {
		const a = pane({}, 1.5);
		const b = pane({}, 1);
		const group = joined(a, b);
		setZoomLocked(group, true);
		zoomed(group);
		expect(b.viewer.followZoom).toHaveBeenCalledWith(1.5, 'custom');
		expect(a.viewer.followZoom).not.toHaveBeenCalled();
	});

	it('follows the pane zoomed, in focus or not, and a fit as a fit', () => {
		const a = pane({}, 1);
		const b = pane({}, 1);
		const group = joined(a, b);
		setZoomLocked(group, true);
		zoomed(group);
		b.zoom = 2;
		zoomed(group);
		expect(a.viewer.followZoom).toHaveBeenLastCalledWith(2, 'custom');
		// What following did is no zoom of a pane's own: nothing more follows it.
		zoomed(group);
		expect(a.viewer.followZoom).toHaveBeenCalledTimes(1);
		expect(b.viewer.followZoom).not.toHaveBeenCalled();
		b.zoomMode = 'fitWidth';
		b.zoom = 1.3;
		zoomed(group);
		expect(a.viewer.followZoom).toHaveBeenLastCalledWith(1.3, 'fitWidth');
		// The fit makes a zoom of its own in each pane, which is not a change to follow.
		b.zoom = 1.31;
		zoomed(group);
		expect(a.viewer.followZoom).toHaveBeenCalledTimes(2);
	});

	it('is not on with synchronized scrolling alone, but for panes of the same document', () => {
		const doc = {};
		const a = pane(doc, 1);
		const b = pane(doc, 1);
		const c = pane({}, 1);
		const group = joined(a, b, c);
		setLocked(group, true);
		zoomed(group);
		a.zoom = 1.25;
		zoomed(group);
		expect(b.viewer.followZoom).toHaveBeenCalledWith(1.25, 'custom');
		expect(c.viewer.followZoom).not.toHaveBeenCalled();
		setLocked(group, false);
		a.zoom = 2;
		zoomed(group);
		expect(b.viewer.followZoom).toHaveBeenCalledTimes(1);
	});

	it('goes off when the panes are no longer side by side', () => {
		const a = pane({}, 1);
		const b = pane({}, 1);
		const group = joined(a, b);
		setZoomLocked(group, true);
		group.setViews([a as unknown as DocTab]);
		expect(group.zoomLocked).toBe(false);
		setZoomLocked(group, true);
		expect(group.zoomLocked).toBe(false);
	});
});
