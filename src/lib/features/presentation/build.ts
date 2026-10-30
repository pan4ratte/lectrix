// A presentation's pages as its screens show them (stage.ts), from the document.

import { hiddenOnPage } from '#lib/features/annotations/actions.ts';
import type { DocTab } from '#lib/stores/doc.svelte.ts';

import type { StageLook, StagePage } from './stage.ts';

/** Page `index` of `tab`, as a presentation shows it. */
export function stagePage(tab: DocTab, index: number): StagePage {
	const size = tab.pages[index]!;
	const hidden = hiddenOnPage(tab, index);
	return {
		index,
		width: size.width,
		height: size.height,
		rotation: size.rotation,
		turn: tab.shownRotation(index),
		hidden,
		layer: tab.annotationLayerOn(index, hidden),
		links: [...(tab.text(index)?.links ?? [])]
	};
}

export function stageLook(tab: DocTab): StageLook {
	return { adjusted: tab.pageFilter !== null, light: tab.pageLight, colour: tab.pageColour, tone: tab.pageTone };
}
