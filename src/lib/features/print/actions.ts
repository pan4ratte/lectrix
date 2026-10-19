// Print (section 6.12): File > Print… (Ctrl+P) opens the Print dialog for the document
// shown, unless its security settings do not allow printing it.

import { app } from '#lib/stores/app.svelte.ts';
import type { DocTab } from '#lib/stores/doc.svelte.ts';

export function openPrint(tab: DocTab) {
	if (tab.flags.printing === 'notAllowed') {
		app.notify(
			{
				kind: 'error',
				message: `${tab.name} doesn’t allow printing.`,
				suggestion: 'Its security settings forbid it. Ask its author for a copy that can be printed.'
			},
			12000
		);
		return;
	}
	app.printRequest = { tabId: tab.id };
}
