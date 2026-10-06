// Applies the appearance chosen in Settings (section 8). Also the WCAG contrast arithmetic
// the token tests use. The colours are Neo's (ADR 0014) with the Lectrix blue accent
// (ADR 0010), fixed in src/app.css.

import type { Appearance } from '#lib/ipc/index.ts';

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

/** The side panes' background, text and accent colours in each theme (the `--lectrix-chrome`,
 * `--lectrix-fg` and `--lectrix-accent` tokens; the contrast test keeps them in step), for
 * colours worked out in code. */
export const PANE_COLORS = {
	light: { pane: '#f2f4f7', fg: '#22252a', accent: '#2f5daa' },
	dark: { pane: '#131416', fg: '#a9b0bc', accent: '#6fa3ef' }
} as const;

/** The share of the accent in a selected row's background (`--lectrix-row-selected`). */
export const ROW_SELECTED_SHARE = 0.16;

/**
 * `hex`, moved toward the text colour just far enough that it shows at 3:1 (WCAG 1.4.11)
 * on a side pane, plain or under a selected row's tint. A colour that already shows stays
 * as it is, so annotation colours keep their own look wherever they can.
 */
export function legibleOnPane(hex: string, dark: boolean): string {
	const { pane, fg, accent } = dark ? PANE_COLORS.dark : PANE_COLORS.light;
	const selected = mix(pane, accent, ROW_SELECTED_SHARE);
	for (let step = 0; step <= 20; step++) {
		const c = mix(hex, fg, step / 20);
		if (contrast(c, pane) >= 3 && contrast(c, selected) >= 3) return c;
	}
	return fg;
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
