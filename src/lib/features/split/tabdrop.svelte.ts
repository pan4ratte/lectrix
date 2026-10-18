// A tab dragged off the row over the pages near an edge (section 6.1, split view): where it
// would join the tab shown. The row (Tabs.svelte) works it out as the pointer moves; the
// pages show it (DropPreview.svelte).

/** Before which pane of the tab shown the dragged tab would go, and how many panes that tab
 * has now; `at` is null while it is not over a place that takes it. */
export const tabDrop = $state<{ at: number | null; panes: number }>({ at: null, panes: 0 });

/** The element the pages of the tab shown are laid out in, registered by it. */
export const dropArea: { element: HTMLElement | null } = { element: null };

/** `{@attach dropTarget}`: this element is where the pages of the tab shown are. */
export function dropTarget(element: HTMLElement) {
	dropArea.element = element;
	return () => {
		if (dropArea.element === element) dropArea.element = null;
	};
}

export function clearDrop() {
	tabDrop.at = null;
	tabDrop.panes = 0;
}
