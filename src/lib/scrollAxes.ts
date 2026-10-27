// Scrollers that may scroll both ways (the start screen, Compress's preview) scroll only along
// an axis their content overflows (docs/spec/ui.md, 8.2). With overflow auto, content a
// fraction of a pixel too wide or tall gives that axis a scroll range, and a touchpad's
// diagonal swipe slides it, or passes the swipe's other half on from a list inside it.

import type { Attachment } from 'svelte/attachments';

/** Overflow up to this much (CSS pixels) is rounding, not content to scroll to. */
const SLACK = 1;

/** `{@attach scrollAxes}`: hides the overflow of each axis the content does not overflow by
 * more than a pixel, as the element or its children change size. */
export const scrollAxes: Attachment<HTMLElement> = (element) => {
	const update = () => {
		const x = element.scrollWidth - element.clientWidth > SLACK;
		const y = element.scrollHeight - element.clientHeight > SLACK;
		element.style.overflowX = x ? '' : 'hidden';
		element.style.overflowY = y ? '' : 'hidden';
	};
	// The element itself (a scrollbar going changes its client size too), and its children,
	// which hold what it scrolls.
	const sizes = new ResizeObserver(update);
	const watch = () => {
		sizes.disconnect();
		sizes.observe(element);
		for (const child of element.children) sizes.observe(child);
		update();
	};
	const children = new MutationObserver(watch);
	children.observe(element, { childList: true });
	watch();
	return () => {
		sizes.disconnect();
		children.disconnect();
		element.style.overflowX = '';
		element.style.overflowY = '';
	};
};
