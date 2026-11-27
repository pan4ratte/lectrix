import { describe, expect, it } from 'vitest';

import { LOOKUP_SITES, SYSTEM_LANGUAGE, lookupLanguage, offeredSites, systemLanguage } from './lookups.ts';

describe('web lookups', () => {
	it('offers the websites switched on in three groups, in the menu’s order', () => {
		const groups = offeredSites(LOOKUP_SITES.map((s) => s.id));
		expect(groups.map((g) => g.sites.map((s) => s.id))).toEqual([
			['googleTranslate', 'deepl'],
			['google', 'duckDuckGo', 'wikipedia'],
			['googleScholar', 'semanticScholar', 'jstor']
		]);
		expect(groups.flatMap((g) => g.sites)).toHaveLength(LOOKUP_SITES.length);
	});

	it('leaves out the websites switched off, and a group with none left', () => {
		const groups = offeredSites(['jstor', 'google', 'wikipedia', 'googleScholar', 'semanticScholar']);
		expect(groups.map((g) => g.sites.map((s) => s.id))).toEqual([
			['google', 'wikipedia'],
			['googleScholar', 'semanticScholar', 'jstor']
		]);
		expect(offeredSites([])).toEqual([]);
	});

	it('translates into the language set, or the system’s', () => {
		expect(lookupLanguage('zh-TW')).toBe('zh-TW');
		expect(lookupLanguage(SYSTEM_LANGUAGE)).toBe(systemLanguage());
		expect(lookupLanguage(undefined)).toBe(lookupLanguage(SYSTEM_LANGUAGE));
	});
});
