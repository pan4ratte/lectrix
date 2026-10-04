// Applies the system look reported at startup: the window backdrop (Mica or solid) and the
// accent color (section 8).

import type { Appearance, StartupInfo } from '#lib/ipc/index.ts';

/** Relative luminance of a #rrggbb color (WCAG 2.x). */
export function luminance(hex: string): number {
	const m = /^#([0-9a-f]{6})$/i.exec(hex);
	if (!m) return 0;
	const n = parseInt(m[1]!, 16);
	const channel = (c: number) => {
		const s = c / 255;
		return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
	};
	return 0.2126 * channel((n >> 16) & 255) + 0.7152 * channel((n >> 8) & 255) + 0.0722 * channel(n & 255);
}

/** WCAG 2.x contrast ratio of two #rrggbb colors (1 to 21). */
export function contrast(a: string, b: string): number {
	const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
	return (hi + 0.05) / (lo + 0.05);
}

/** `hex` moved toward `toward` by `amount` (0 to 1), mixed in sRGB like CSS color-mix. */
export function mix(hex: string, toward: string, amount: number): string {
	const parse = (h: string) => {
		const n = parseInt(/^#([0-9a-f]{6})$/i.exec(h)?.[1] ?? '000000', 16);
		return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
	};
	const a = parse(hex);
	const b = parse(toward);
	return `#${a
		.map((c, i) => Math.round(c + (b[i]! - c) * amount))
		.map((c) => c.toString(16).padStart(2, '0'))
		.join('')}`;
}

/** Black or white, whichever reads better on `hex` (WCAG contrast). */
export function textOn(hex: string): '#000000' | '#ffffff' {
	return contrast(hex, '#ffffff') >= contrast(hex, '#000000') ? '#ffffff' : '#000000';
}

/**
 * The backgrounds accent marks sit on (focus rings, selected tabs, outlines): surface,
 * chrome, raised surface and canvas, as in src/app.css (checked by theme.test.ts).
 */
export const LIGHT_SURFACES = ['#fbfbfb', '#f3f3f3', '#ffffff', '#e6e6e6'];
export const DARK_SURFACES = ['#2b2b2b', '#202020', '#2c2c2c', '#1a1a1a'];

/** Contrast that marks and outlines need against what surrounds them (WCAG 1.4.11). */
const MARK_CONTRAST = 3;

function shadeFor(start: string, toward: string, surfaces: string[]): string {
	let shade = start;
	for (let step = 1; step <= 20 && surfaces.some((s) => contrast(shade, s) < MARK_CONTRAST); step++) {
		shade = mix(start, toward, step * 0.05);
	}
	return shade;
}

export interface AccentShades {
	light: string;
	lightFg: string;
	dark: string;
	darkFg: string;
}

/**
 * The system accent as the light and dark themes use it. A pale accent (gold, say) would
 * be hard to see as a focus ring on white, so each shade is darkened (light theme) or
 * lightened (dark theme) until it has 3:1 against every surface; text on it is black or
 * white, whichever reads better. Dark starts from a lighter tint, as Windows does.
 */
export function accentShades(accent: string): AccentShades {
	const light = shadeFor(accent, '#000000', LIGHT_SURFACES);
	const dark = shadeFor(mix(accent, '#ffffff', 0.45), '#ffffff', DARK_SURFACES);
	return { light, lightFg: textOn(light), dark, darkFg: textOn(dark) };
}

/**
 * Light, dark or the system's choice (Settings). Rust also sets the window's theme; the
 * attribute makes the design tokens follow even where the webview keeps reporting the
 * system's scheme.
 */
export function applyAppearance(appearance: Appearance) {
	const root = document.documentElement;
	if (appearance === 'system') delete root.dataset.theme;
	else root.dataset.theme = appearance;
}

export function applyTheme(info: StartupInfo) {
	const root = document.documentElement;
	root.dataset.backdrop = info.backdrop;
	if (info.accentColor) {
		const shades = accentShades(info.accentColor);
		root.style.setProperty('--lectrix-system-accent', shades.light);
		root.style.setProperty('--lectrix-system-accent-fg', shades.lightFg);
		root.style.setProperty('--lectrix-system-accent-dark', shades.dark);
		root.style.setProperty('--lectrix-system-accent-dark-fg', shades.darkFg);
	}
}
