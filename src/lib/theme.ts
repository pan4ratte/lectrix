// Applies the system look reported at startup: the window backdrop (Mica or solid) and the
// accent color (section 8).

import type { StartupInfo } from '#lib/ipc/index.ts';

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

/** Black or white, whichever reads better on `hex` (WCAG contrast). */
export function textOn(hex: string): '#000000' | '#ffffff' {
	const l = luminance(hex);
	const contrastWhite = 1.05 / (l + 0.05);
	const contrastBlack = (l + 0.05) / 0.05;
	return contrastWhite >= contrastBlack ? '#ffffff' : '#000000';
}

export function applyTheme(info: StartupInfo) {
	const root = document.documentElement;
	root.dataset.backdrop = info.backdrop;
	if (info.accentColor) {
		root.style.setProperty('--folio-system-accent', info.accentColor);
		root.style.setProperty('--folio-system-accent-fg', textOn(info.accentColor));
	}
}
