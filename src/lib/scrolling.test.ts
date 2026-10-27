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
