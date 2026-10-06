// Dates and sizes as the start screen's recent files show them, in the user's locale.

/**
 * When a file was opened, as short as a list needs: "Today, 19:07", "Yesterday, 22:16",
 * then the day and month this year ("Oct 4"), and the year too before that.
 */
export function openedDate(ms: number, now: number = Date.now(), locale?: string): string {
	const d = new Date(ms);
	const today = new Date(now);
	const yesterday = new Date(today.getFullYear(), today.getMonth(), today.getDate() - 1);
	// The hour unpadded ("0:52"), as Windows writes short times; 12-hour locales keep AM/PM.
	const time = () => d.toLocaleTimeString(locale, { hour: 'numeric', minute: '2-digit' });
	if (d.toDateString() === today.toDateString()) return `Today, ${time()}`;
	if (d.toDateString() === yesterday.toDateString()) return `Yesterday, ${time()}`;
	if (d.getFullYear() === today.getFullYear()) {
		return d.toLocaleDateString(locale, { day: 'numeric', month: 'short' });
	}
	return d.toLocaleDateString(locale, { day: 'numeric', month: 'short', year: 'numeric' });
}

/** The full date and time, for a tooltip. */
export function fullDate(ms: number, locale?: string): string {
	return new Date(ms).toLocaleString(locale, { dateStyle: 'full', timeStyle: 'short' });
}

const UNITS = ['KB', 'MB', 'GB', 'TB'];

/**
 * A file size as File Explorer counts it (1 KB = 1024 bytes): "812 bytes", "48 KB",
 * "3.4 MB"; one decimal below 10, whole numbers above.
 */
export function fileSize(bytes: number, locale?: string): string {
	if (bytes < 1024) return bytes === 1 ? '1 byte' : `${bytes.toLocaleString(locale)} bytes`;
	let value = bytes / 1024;
	let unit = 0;
	while (value >= 1024 && unit < UNITS.length - 1) {
		value /= 1024;
		unit++;
	}
	const digits = value < 10 ? 1 : 0;
	return `${value.toLocaleString(locale, { maximumFractionDigits: digits })} ${UNITS[unit]}`;
}
