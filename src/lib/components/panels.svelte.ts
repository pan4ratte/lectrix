// The panels of the side panes (section 8) and dragging them: along a pane's row, into the
// other pane's row, or onto a closed or empty pane's place in the view bar row. Pointer
// events, not HTML drag and drop, which the webview keeps for files dropped from Explorer.

import { Bookmark, GalleryVertical, MessagesSquare, Tag } from '@lucide/svelte';

import type { PanelId } from '#lib/ipc/index.ts';
import { app } from '#lib/stores/app.svelte.ts';

import type { PaneSide } from './panes.ts';

export const PANEL_INFO = {
	pages: { label: 'Pages', icon: GalleryVertical },
	bookmarks: { label: 'Bookmarks', icon: Bookmark },
	labels: { label: 'Page labels', icon: Tag },
	annotations: { label: 'Annotations', icon: MessagesSquare }
} as const satisfies Record<PanelId, { label: string; icon: unknown }>;

/** Where a dragged panel would land: before the `slot`-th tab of a pane's row as shown. */
export interface PanelDrop {
	side: PaneSide;
	slot: number;
}

interface PanelDragState {
	panel: PanelId;
	startX: number;
	startY: number;
	x: number;
	y: number;
	/** Past the threshold: a drag, not a click. */
	moving: boolean;
	drop: PanelDrop | null;
}

/** Pointer travel before a press on a panel's tab becomes a drag. */
const DRAG_THRESHOLD = 6;

export const panelDrag: { current: PanelDragState | null } = $state({ current: null });

/** A press on a panel's tab: a drag starts once the pointer has moved far enough. */
export function startPanelDrag(panel: PanelId, event: PointerEvent) {
	if (event.button !== 0 || event.pointerType === 'touch') return;
	panelDrag.current = {
		panel,
		startX: event.clientX,
		startY: event.clientY,
		x: event.clientX,
		y: event.clientY,
		moving: false,
		drop: null
	};
	window.addEventListener('pointermove', onMove, true);
	window.addEventListener('pointerup', onUp, true);
	window.addEventListener('pointercancel', end, true);
	window.addEventListener('keydown', onKey, true);
	window.addEventListener('blur', end);
}

function onMove(event: PointerEvent) {
	const drag = panelDrag.current;
	if (!drag) return;
	drag.x = event.clientX;
	drag.y = event.clientY;
	if (!drag.moving && Math.hypot(drag.x - drag.startX, drag.y - drag.startY) < DRAG_THRESHOLD) return;
	drag.moving = true;
	drag.drop = findDrop(drag.x, drag.y);
}

function onUp() {
	const drag = panelDrag.current;
	end();
	if (drag?.moving && drag.drop) dropPanel(drag.panel, drag.drop);
}

function onKey(event: KeyboardEvent) {
	if (event.key !== 'Escape' || !panelDrag.current?.moving) return;
	event.preventDefault();
	event.stopPropagation();
	end();
}

function end() {
	panelDrag.current = null;
	window.removeEventListener('pointermove', onMove, true);
	window.removeEventListener('pointerup', onUp, true);
	window.removeEventListener('pointercancel', end, true);
	window.removeEventListener('keydown', onKey, true);
	window.removeEventListener('blur', end);
}

/**
 * The drop place under the pointer. Drop zones carry `data-panel-drop` with their side: a
 * pane's row, where the tabs (`data-panel-tab`) give the slot, or a closed or empty pane's
 * place, which takes the panel at its end.
 */
function findDrop(x: number, y: number): PanelDrop | null {
	for (const zone of document.querySelectorAll<HTMLElement>('[data-panel-drop]')) {
		const r = zone.getBoundingClientRect();
		if (x < r.left || x > r.right || y < r.top || y > r.bottom) continue;
		const side = zone.dataset.panelDrop === 'left' ? 'left' : 'right';
		const tabs = [...zone.querySelectorAll<HTMLElement>('[data-panel-tab]')];
		if (tabs.length === 0) return { side, slot: app.panels[side].length };
		const slot = tabs.findIndex((t) => {
			const tr = t.getBoundingClientRect();
			return x < tr.left + tr.width / 2;
		});
		return { side, slot: slot < 0 ? tabs.length : slot };
	}
	return null;
}

function dropPanel(panel: PanelId, drop: PanelDrop) {
	const list = app.panels[drop.side];
	const from = list.indexOf(panel);
	// The slot counts the dragged tab when it is in this row; the move counts without it.
	const index = from >= 0 && from < drop.slot ? drop.slot - 1 : drop.slot;
	if (from === index) return;
	app.movePanel(panel, drop.side, index);
}

/** The dragged panel, while a drag is under way. */
export function draggedPanel(): PanelId | null {
	return panelDrag.current?.moving ? panelDrag.current.panel : null;
}

/** The slot a drag would drop into in a pane's row, or null. */
export function dropSlot(side: PaneSide): number | null {
	const drag = panelDrag.current;
	return drag?.moving && drag.drop?.side === side ? drag.drop.slot : null;
}
