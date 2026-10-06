// Colour arithmetic for the custom colour picker (section 6.5): hex values as typed, and the
// hue, saturation and brightness its square and hue bar show.

/** Hue 0 to 360, saturation and brightness (value) 0 to 1. */
export interface Hsv {
	h: number;
	s: number;
	v: number;
}

/**
 * The colour a hex value names, as #rrggbb in lower case; null if it is not one. Takes six
 * or three digits, with or without the #, and space around them.
 */
export function parseHex(text: string): string | null {
	const m = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(text.trim());
	if (!m) return null;
	const digits = m[1]!.toLowerCase();
	return `#${digits.length === 3 ? [...digits].map((d) => d + d).join('') : digits}`;
}

export function hexToHsv(hex: string): Hsv {
	const n = parseInt(hex.slice(1), 16);
	const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((c) => c / 255) as [number, number, number];
	const max = Math.max(r, g, b);
	const delta = max - Math.min(r, g, b);
	let h = 0;
	if (delta > 0) {
		if (max === r) h = ((g - b) / delta) % 6;
		else if (max === g) h = (b - r) / delta + 2;
		else h = (r - g) / delta + 4;
		h = (h * 60 + 360) % 360;
	}
	return { h, s: max === 0 ? 0 : delta / max, v: max };
}

export function hsvToHex({ h, s, v }: Hsv): string {
	const channel = (k: number) => {
		const x = (k + h / 60) % 6;
		return v - v * s * Math.max(0, Math.min(x, 4 - x, 1));
	};
	return `#${[channel(5), channel(3), channel(1)]
		.map((c) => Math.round(c * 255).toString(16).padStart(2, '0'))
		.join('')}`;
}
