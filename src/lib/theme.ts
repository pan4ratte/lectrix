// Applies the system look reported at startup, the window backdrop (Mica or solid), and the
// appearance chosen in Settings (section 8). Also the WCAG contrast arithmetic the token
// tests use. The accent is Lectrix blue, fixed in src/app.css (ADR 0010).

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
	document.documentElement.dataset.backdrop = info.backdrop;
}
