// WCAG AA contrast of the design tokens in src/app.css (AGENTS.md section 8,
// accessibility): text 4.5:1, focus rings and marks 3:1 (WCAG 1.4.3 and 1.4.11), in
// the light and the dark theme, with the Lectrix blue accent (ADR 0010).
import { readFileSync } from 'node:fs';

import { describe, expect, it } from 'vitest';

import { PRESET_COLORS } from './features/annotations/tools';
import { PANE_COLORS, ROW_SELECTED_SHARE, contrast, legibleOnPane, mix } from './theme';

const css = readFileSync(new URL('../app.css', import.meta.url), 'utf8');

/** The `--lectrix-*` declarations of the first rule whose selector is exactly `selector`. */
function tokens(selector: string): Record<string, string> {
	const start = css.indexOf(`${selector} {`);
	if (start < 0) throw new Error(`no rule ${selector}`);
	const body = css.slice(start, css.indexOf('}', start));
	const out: Record<string, string> = {};
	for (const m of body.matchAll(/(--lectrix-[\w-]+):\s*([^;]+);/g)) out[m[1]!] = m[2]!.trim();
	return out;
}

const light = tokens(':root');
const dark = { ...light, ...tokens(":root[data-theme='dark']") };

interface Rgba {
	hex: string;
	alpha: number;
}

/** Splits `a, b, c` at top-level commas. */
function args(s: string): string[] {
	const out: string[] = [];
	let depth = 0;
	let cur = '';
	for (const ch of s) {
		if (ch === '(') depth++;
		if (ch === ')') depth--;
		if (ch === ',' && depth === 0) {
			out.push(cur.trim());
			cur = '';
		} else cur += ch;
	}
	out.push(cur.trim());
	return out;
}

/** Evaluates a token value: hex, rgb() with alpha, var() with fallback, color-mix(). */
function evaluate(value: string, vars: Record<string, string>): Rgba {
	const v = value.trim();
	if (/^#[0-9a-f]{6}$/i.test(v)) return { hex: v.toLowerCase(), alpha: 1 };
	if (v === 'white') return { hex: '#ffffff', alpha: 1 };
	if (v === 'black') return { hex: '#000000', alpha: 1 };
	if (v === 'transparent') return { hex: '#000000', alpha: 0 };
	let m = /^var\((--[\w-]+)(?:,\s*(.+))?\)$/.exec(v);
	if (m) {
		const own = vars[m[1]!];
		if (own !== undefined) return evaluate(own, vars);
		if (m[2] !== undefined) return evaluate(m[2], vars);
		throw new Error(`undefined ${m[1]}`);
	}
	m = /^rgb\((\d+) (\d+) (\d+) \/ ([\d.]+)\)$/.exec(v);
	if (m) {
		const hex = `#${[m[1], m[2], m[3]].map((c) => Number(c).toString(16).padStart(2, '0')).join('')}`;
		return { hex, alpha: Number(m[4]) };
	}
	m = /^color-mix\(in srgb,\s*(.+)\)$/.exec(v);
	if (m) {
		const [first, second] = args(m[1]!) as [string, string];
		const pm = /^(.+?)\s+([\d.]+)%$/.exec(first);
		const a = evaluate(pm ? pm[1]! : first, vars);
		const share = pm ? Number(pm[2]) / 100 : 0.5;
		const b = evaluate(second, vars);
		if (b.alpha === 0) return { hex: a.hex, alpha: a.alpha * share };
		return { hex: mix(a.hex, b.hex, 1 - share), alpha: 1 };
	}
	throw new Error(`cannot evaluate ${v}`);
}

/** The token as it shows on `background` (translucent tokens composited onto it). */
function on(name: string, vars: Record<string, string>, background: string): string {
	const c = evaluate(`var(${name})`, vars);
	return c.alpha >= 1 ? c.hex : mix(background, c.hex, c.alpha);
}

const solid = (name: string, vars: Record<string, string>) => {
	const c = evaluate(`var(${name})`, vars);
	expect(c.alpha, `${name} is opaque`).toBe(1);
	return c.hex;
};

describe.each([
	['light', light],
	['dark', dark]
])('%s theme', (_name, vars) => {
	const surfaces = ['--lectrix-bg', '--lectrix-chrome', '--lectrix-surface', '--lectrix-surface-raised', '--lectrix-menu', '--lectrix-canvas'];
	// Filled controls: text on them, with no hover or selection tint on top.
	const fills = ['--lectrix-tab-active', '--lectrix-button', '--lectrix-button-hover'];

	it('text reads at 4.5:1 on every surface, also hovered or selected', () => {
		for (const surface of surfaces) {
			const bg = solid(surface, vars);
			for (const text of ['--lectrix-fg', '--lectrix-fg-muted', '--lectrix-fg-strong']) {
				expect(contrast(solid(text, vars), bg), `${text} on ${surface}`).toBeGreaterThanOrEqual(4.5);
				for (const state of ['--lectrix-hover', '--lectrix-pressed', '--lectrix-row-selected']) {
					const stateBg = on(state, vars, bg);
					expect(contrast(solid(text, vars), stateBg), `${text} on ${state} over ${surface}`).toBeGreaterThanOrEqual(4.5);
				}
			}
		}
		for (const fill of fills) {
			for (const text of ['--lectrix-fg', '--lectrix-fg-muted']) {
				expect(contrast(solid(text, vars), solid(fill, vars)), `${text} on ${fill}`).toBeGreaterThanOrEqual(4.5);
			}
		}
	});

	it('notices, errors and accent buttons read at 4.5:1', () => {
		const fg = solid('--lectrix-fg', vars);
		expect(contrast(fg, solid('--lectrix-info-bg', vars))).toBeGreaterThanOrEqual(4.5);
		expect(contrast(fg, solid('--lectrix-danger-bg', vars))).toBeGreaterThanOrEqual(4.5);
		for (const bg of ['--lectrix-surface', '--lectrix-surface-raised', '--lectrix-danger-bg']) {
			expect(contrast(solid('--lectrix-danger', vars), solid(bg, vars)), `danger on ${bg}`).toBeGreaterThanOrEqual(4.5);
		}
		expect(contrast(solid('--lectrix-accent-fg', vars), solid('--lectrix-accent', vars))).toBeGreaterThanOrEqual(4.5);
		expect(contrast(solid('--lectrix-close-hover-fg', vars), solid('--lectrix-close-hover', vars))).toBeGreaterThanOrEqual(4.5);
	});

	it('focus rings, accent marks and file marks stand out at 3:1', () => {
		for (const surface of surfaces) {
			const bg = solid(surface, vars);
			expect(contrast(solid('--lectrix-focus', vars), bg), `focus on ${surface}`).toBeGreaterThanOrEqual(3);
			expect(contrast(solid('--lectrix-accent', vars), bg), `accent on ${surface}`).toBeGreaterThanOrEqual(3);
		}
		// The active tool's icon and border, on its accent tint over the toolbar (floating, or
		// docked in the view bar, which is the panes' colour); also the icon of the Settings
		// group shown, on the same tint over the panes' colour.
		for (const bar of ['--lectrix-surface', '--lectrix-surface-raised', '--lectrix-chrome']) {
			const tint = on('--lectrix-row-selected', vars, solid(bar, vars));
			expect(contrast(solid('--lectrix-accent', vars), tint), `active tool on ${bar}`).toBeGreaterThanOrEqual(3);
		}
		// The start screen's tools: borders, icons and text in the brand gradient, on the canvas
		// and on the wash of it under the pointer; text needs 4.5:1.
		const canvas = solid('--lectrix-canvas', vars);
		const wash = Number(vars['--lectrix-brand-wash']);
		expect(wash).toBeGreaterThan(0);
		for (const end of ['--lectrix-brand-from', '--lectrix-brand-to']) {
			const brand = solid(end, vars);
			expect(contrast(brand, canvas), `${end} text on the canvas`).toBeGreaterThanOrEqual(4.5);
			expect(contrast(brand, mix(canvas, brand, wash)), `${end} text on its wash`).toBeGreaterThanOrEqual(4.5);
		}
		for (let i = 1; i <= 6; i++) {
			for (const bg of ['--lectrix-surface', '--lectrix-chrome']) {
				expect(contrast(solid(`--lectrix-source-${i}`, vars), solid(bg, vars)), `source ${i} on ${bg}`).toBeGreaterThanOrEqual(3);
			}
		}
	});

	it('annotation icons in the list show at 3:1, keeping colours that already do', () => {
		const theme = _name === 'dark' ? PANE_COLORS.dark : PANE_COLORS.light;
		// The colours worked out in code are the tokens.
		expect(theme.pane).toBe(solid('--lectrix-chrome', vars));
		expect(theme.fg).toBe(solid('--lectrix-fg', vars));
		expect(theme.accent).toBe(solid('--lectrix-accent', vars));
		expect(on('--lectrix-row-selected', vars, theme.pane)).toBe(mix(theme.pane, theme.accent, ROW_SELECTED_SHARE));
		const selected = mix(theme.pane, theme.accent, ROW_SELECTED_SHARE);
		for (const { value, name } of PRESET_COLORS) {
			const shown = legibleOnPane(value, _name === 'dark');
			expect(contrast(shown, theme.pane), `${name} on the pane`).toBeGreaterThanOrEqual(3);
			expect(contrast(shown, selected), `${name} on a selected row`).toBeGreaterThanOrEqual(3);
			if (contrast(value, theme.pane) >= 3 && contrast(value, selected) >= 3) expect(shown, name).toBe(value);
		}
	});
});
