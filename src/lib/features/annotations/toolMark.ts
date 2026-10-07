// The mark under the active tool (section 8): one accent ring over a tint of the accent,
// shared by a group of tool buttons, that glides to the tool picked instead of jumping
// there. Used by the annotation toolbar, by the type buttons of the annotation bar and the
// inspector, and by the side panes' panel tabs. The group holds a `.tool-mark` span first,
// then its `.tool-button`s; the pressed one (aria-pressed), or the selected tab
// (aria-selected), is where the mark goes.

import type { Attachment } from 'svelte/attachments';

/** How long the mark glides to the tool picked, as zooming does (section 6.1). */
export const GLIDE_MS = 140;

const CHOSEN = ':scope > .tool-button:is([aria-pressed="true"], [aria-selected="true"])';

export const toolMark: Attachment<HTMLElement> = (group) => {
	const mark = group.querySelector<HTMLElement>(':scope > .tool-mark');
	if (!mark) return;
	let shown = false;

	/** Puts the mark on the chosen tool: gliding there from the one before, or at once. */
	function place(glide: boolean) {
		const chosen = group.querySelector<HTMLElement>(CHOSEN);
		if (!mark) return;
		if (!chosen) {
			mark.hidden = true;
			shown = false;
			return;
		}
		// The duration in the same style change as the move: that move glides, or doesn't.
		// Reduced motion turns every transition off (app.css).
		mark.style.transitionDuration = glide && shown ? `${GLIDE_MS}ms` : '0ms';
		mark.style.transform = `translate(${chosen.offsetLeft}px, ${chosen.offsetTop}px)`;
		mark.style.width = `${chosen.offsetWidth}px`;
		mark.style.height = `${chosen.offsetHeight}px`;
		mark.hidden = false;
		shown = true;
	}

	place(false);
	// A tool picked, a tab chosen, or an annotation's type changed: glide.
	const picked = new MutationObserver(() => place(true));
	picked.observe(group, { subtree: true, attributes: true, attributeFilter: ['aria-pressed', 'aria-selected'] });
	// The buttons laid out again (the bar appearing, a wrap, a tab moved): follow at once.
	const moved = new MutationObserver(() => place(false));
	moved.observe(group, { childList: true });
	const resized = new ResizeObserver(() => place(false));
	resized.observe(group);
	return () => {
		picked.disconnect();
		moved.disconnect();
		resized.disconnect();
	};
};
