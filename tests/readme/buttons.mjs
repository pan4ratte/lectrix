// The README's Download and Support buttons (docs/images/button-*.svg):
//
//     node tests/readme/buttons.mjs
//
// shieldcn.dev draws them (its text as outlines, so they look the same everywhere), and
// this gives them Lectrix's gradient in place of its one colour, which shieldcn can't do:
// from the deep blue of the app icon's lower corner to the brand's accent (ADR 0010). Run it
// again after changing a button's text.

import { writeFileSync } from 'node:fs';
import { join } from 'node:path';

import { ROOT } from '../e2e/lib/app.mjs';

const GRADIENT = ['#4d84e1', '#2f5daa'];
/** shieldcn's colour for the button, which the gradient replaces. */
const FLAT = '2F5DAA';

const BUTTONS = {
	download: { text: 'download for-Windows, macOS, Linux', icon: 'ri:TbDownload' },
	support: { text: 'Поддержать разработку-(RU карты)', icon: 'ri:FaHeart' }
};

for (const [name, { text, icon }] of Object.entries(BUTTONS)) {
	const url = `https://shieldcn.dev/badge/${encodeURIComponent(text)}.svg?size=lg&logo=${encodeURIComponent(icon)}&color=${FLAT}`;
	const res = await fetch(url, { headers: { 'user-agent': 'Mozilla/5.0' } });
	if (!res.ok) throw new Error(`${url}: ${res.status} ${res.statusText}`);
	const svg = await res.text();
	const fill = `fill="#${FLAT.toLowerCase()}"`;
	if (svg.split(fill).length !== 2) throw new Error(`${name}: shieldcn's button no longer has one ${fill}`);
	const defs =
		`<defs><linearGradient id="lectrix" x1="0" y1="0" x2="1" y2="1">` +
		`<stop offset="0" stop-color="${GRADIENT[0]}"/><stop offset="1" stop-color="${GRADIENT[1]}"/>` +
		`</linearGradient></defs>`;
	const branded = svg.replace(/(<svg[^>]*>)/, `$1${defs}`).replace(fill, 'fill="url(#lectrix)"');
	const out = join(ROOT, 'docs', 'images', `button-${name}.svg`);
	writeFileSync(out, branded);
	console.log(`wrote ${out}`);
}
