import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { describe, expect, it } from 'vitest';

// With overflow-y alone the browser makes overflow-x auto too, so content a pixel too wide
// slides sideways under a touchpad's diagonal swipe. A scroller meant to scroll only up and
// down says overflow-x hidden as well.
describe('a scroller that scrolls only up and down', () => {
	const src = fileURLToPath(new URL('..', import.meta.url));

	it('never scrolls sideways (components)', () => {
		const files = readdirSync(src, { recursive: true, encoding: 'utf8' }).filter((name) => name.endsWith('.svelte'));
		expect(files.length).toBeGreaterThan(20);
		const missing = files.flatMap((name) =>
			[...readFileSync(join(src, name), 'utf8').matchAll(/class="([^"]*)"/g)]
				.map((m) => m[1]!)
				.filter((cls) => /(^|\s)overflow-y-auto(\s|$)/.test(cls) && !/(^|\s)overflow-x-hidden(\s|$)/.test(cls))
				.map((cls) => `${name}: ${cls}`)
		);
		expect(missing).toEqual([]);
	});

	it('never scrolls sideways (app.css)', () => {
		const css = readFileSync(join(src, 'app.css'), 'utf8');
		const rules = [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)];
		const missing = rules
			.filter(([, , body]) => /overflow-y:\s*auto/.test(body!) && !/overflow-x:\s*hidden/.test(body!))
			.map(([, selector]) => selector!.trim());
		expect(missing).toEqual([]);
	});
});

/** Each opening tag in a component whose class says `cls`: the tag's text, from `<` to its
 * `>` (skipping any inside `{...}` expressions). */
function tagsWithClass(source: string, cls: RegExp): string[] {
	const tags: string[] = [];
	for (const m of source.matchAll(/class="([^"]*)"/g)) {
		if (!cls.test(m[1]!)) continue;
		const start = source.lastIndexOf('<', m.index);
		let depth = 0;
		let end = start;
		for (; end < source.length; end++) {
			const c = source[end];
			if (c === '{') depth++;
			else if (c === '}') depth--;
			else if (c === '>' && depth === 0) break;
		}
		tags.push(source.slice(start, end + 1));
	}
	return tags;
}

// Overflow auto lets a scroller scroll either way, and content a fraction of a pixel too
// wide or tall gives that axis a range too. A scroller that may scroll both ways hides the
// overflow of an axis its content does not overflow (scrollAxes, or its own style).
describe('a scroller that may scroll both ways', () => {
	const src = fileURLToPath(new URL('..', import.meta.url));

	it('scrolls only along an axis its content overflows (components)', () => {
		const files = readdirSync(src, { recursive: true, encoding: 'utf8' }).filter((name) => name.endsWith('.svelte'));
		const missing = files.flatMap((name) =>
			tagsWithClass(readFileSync(join(src, name), 'utf8'), /(^|\s)overflow-auto(\s|$)/)
				.filter((tag) => !tag.includes('{@attach scrollAxes}') && !(tag.includes('style:overflow-x=') && tag.includes('style:overflow-y=')))
				.map((tag) => `${name}: ${tag.slice(0, 80)}`)
		);
		expect(missing).toEqual([]);
	});

	it('is not made in app.css', () => {
		const css = readFileSync(join(src, 'app.css'), 'utf8');
		const rules = [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)];
		const both = rules.filter(([, , body]) => /(^|[;\s])overflow:\s*(auto|scroll)/.test(body!)).map(([, selector]) => selector!.trim());
		expect(both).toEqual([]);
	});

	it('finds the tags it checks', () => {
		const tag = '<div\n\tclass="a overflow-auto"\n\tonclick={() => (x = y > 1)}\n\t{@attach scrollAxes}\n>';
		expect(tagsWithClass(`${tag}<span class="b"></span>`, /overflow-auto/)).toEqual([tag]);
	});
});
