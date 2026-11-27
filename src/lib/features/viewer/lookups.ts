// Looking selected text up on the web (section 6.1, ADR 0037): the websites the right-click
// menu of selected text offers, each switched on or off in Settings. Rust builds the
// address and hands it to the browser.

import type { WebLookup } from '#lib/ipc/index.ts';

export interface LookupSite {
	id: WebLookup;
	/** The menu item's and the Settings row's label. */
	label: string;
	/** The menu shows the groups with a line between them. */
	group: 'translate' | 'search' | 'scholar';
}

/** In the menu's order. */
export const LOOKUP_SITES: readonly LookupSite[] = [
	{ id: 'googleTranslate', label: 'Translate with Google', group: 'translate' },
	{ id: 'deepl', label: 'Translate with DeepL', group: 'translate' },
	{ id: 'google', label: 'Search Google', group: 'search' },
	{ id: 'duckDuckGo', label: 'Search DuckDuckGo', group: 'search' },
	{ id: 'wikipedia', label: 'Search Wikipedia', group: 'search' },
	{ id: 'googleScholar', label: 'Search Google Scholar', group: 'scholar' },
	{ id: 'semanticScholar', label: 'Search Semantic Scholar', group: 'scholar' },
	{ id: 'jstor', label: 'Search JSTOR', group: 'scholar' }
];

/** The websites the menu offers until Settings are loaded (mirrors WebLookup::DEFAULT in
 * Rust). */
export const DEFAULT_WEB_LOOKUPS: readonly WebLookup[] = ['deepl', 'google', 'googleScholar'];

/** The websites the menu offers, in groups (none empty): those of `on`, in the menu's order. */
export function offeredSites(on: readonly WebLookup[]): { group: LookupSite['group']; sites: LookupSite[] }[] {
	const groups: { group: LookupSite['group']; sites: LookupSite[] }[] = [];
	for (const site of LOOKUP_SITES) {
		if (!on.includes(site.id)) continue;
		const last = groups.at(-1);
		if (last?.group === site.group) last.sites.push(site);
		else groups.push({ group: site.group, sites: [site] });
	}
	return groups;
}

/** The languages Settings offers to translate into (each one Google Translate and DeepL
 * both translate into), by their English names, and their tags. */
export const LOOKUP_LANGUAGES: readonly { value: string; label: string }[] = [
	{ value: 'ar', label: 'Arabic' },
	{ value: 'bg', label: 'Bulgarian' },
	{ value: 'zh-CN', label: 'Chinese (Simplified)' },
	{ value: 'zh-TW', label: 'Chinese (Traditional)' },
	{ value: 'cs', label: 'Czech' },
	{ value: 'da', label: 'Danish' },
	{ value: 'nl', label: 'Dutch' },
	{ value: 'en', label: 'English' },
	{ value: 'et', label: 'Estonian' },
	{ value: 'fi', label: 'Finnish' },
	{ value: 'fr', label: 'French' },
	{ value: 'de', label: 'German' },
	{ value: 'el', label: 'Greek' },
	{ value: 'he', label: 'Hebrew' },
	{ value: 'hu', label: 'Hungarian' },
	{ value: 'id', label: 'Indonesian' },
	{ value: 'it', label: 'Italian' },
	{ value: 'ja', label: 'Japanese' },
	{ value: 'ko', label: 'Korean' },
	{ value: 'lv', label: 'Latvian' },
	{ value: 'lt', label: 'Lithuanian' },
	{ value: 'nb', label: 'Norwegian' },
	{ value: 'pl', label: 'Polish' },
	{ value: 'pt', label: 'Portuguese' },
	{ value: 'ro', label: 'Romanian' },
	{ value: 'ru', label: 'Russian' },
	{ value: 'sk', label: 'Slovak' },
	{ value: 'sl', label: 'Slovenian' },
	{ value: 'es', label: 'Spanish' },
	{ value: 'sv', label: 'Swedish' },
	{ value: 'th', label: 'Thai' },
	{ value: 'tr', label: 'Turkish' },
	{ value: 'uk', label: 'Ukrainian' },
	{ value: 'vi', label: 'Vietnamese' }
];

/** The language setting's value that follows the system (`SYSTEM_LANGUAGE` in Rust). */
export const SYSTEM_LANGUAGE = 'system';

/** The system's language, as the webview has it ("ru-RU"). */
export function systemLanguage(): string {
	return typeof navigator === 'undefined' ? 'en' : (navigator.languages?.[0] ?? navigator.language ?? 'en');
}

/** The system's language by its English name ("Russian"), for Settings to say. */
export function systemLanguageName(): string {
	const tag = systemLanguage();
	try {
		return new Intl.DisplayNames(['en'], { type: 'language' }).of(tag) ?? tag;
	} catch {
		return tag;
	}
}

/** The language lookups translate into and search Wikipedia in, from the setting. */
export function lookupLanguage(setting: string | undefined): string {
	return !setting || setting === SYSTEM_LANGUAGE ? systemLanguage() : setting;
}
