// The selection of a page grid (`PageGrid.svelte`): which pages are selected, which has the
// keyboard focus, and where a Shift+click extends from. The Combine view and the page
// organizing tool build their state on it.

import { rangeOfKeys } from './pagegrid.ts';

export abstract class GridSelection {
	/** Keys of the selected pages. */
	selected = $state<number[]>([]);
	selectedSet = $derived(new Set(this.selected));
	/** The page with keyboard focus, also the anchor for Shift+click and Shift+arrows. */
	focusKey = $state<number | null>(null);
	private anchor: number | null = null;

	/** The pages' keys, in the order the grid shows them. */
	protected abstract keys(): number[];

	/** Click: `single` selects one page, `toggle` (Ctrl) adds or removes it, `range` (Shift) selects from the anchor. */
	select(key: number, mode: 'single' | 'toggle' | 'range') {
		if (mode === 'range' && this.anchor !== null) {
			this.selected = rangeOfKeys(this.keys(), this.anchor, key);
		} else if (mode === 'toggle') {
			this.selected = this.selectedSet.has(key)
				? this.selected.filter((k) => k !== key)
				: [...this.selected, key];
			this.anchor = key;
		} else {
			this.selected = [key];
			this.anchor = key;
		}
		this.focusKey = key;
	}

	selectAll() {
		this.selected = this.keys();
	}

	/** Replaces the selection (a rectangle drawn over the grid); `focus`, if any, gets the
	 * keyboard focus and anchors the next Shift+click. */
	setSelection(keys: number[], focus: number | null) {
		this.selected = keys;
		if (focus !== null) {
			this.focusKey = focus;
			this.anchor = focus;
		}
	}
}
