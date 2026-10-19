// A document's properties (section 6.10): Document > Properties… (Ctrl+D), or the lock in a
// protected document's tab, which opens them at Security.

import { app, type PropertiesSection } from '#lib/stores/app.svelte.ts';
import type { DocTab } from '#lib/stores/doc.svelte.ts';

export function openProperties(tab: DocTab, section: PropertiesSection = 'description') {
	app.propertiesRequest = { tabId: tab.id, section };
}
