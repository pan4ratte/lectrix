// The platform the app runs on, for what looks or is placed as that platform's own apps have
// it: the title bar and its window buttons, and the dialogs on macOS (section 8). Read from
// the webview's user agent, so it is known before anything is drawn: WebView2 on Windows,
// WKWebView on macOS and WebKitGTK on Linux name their system there.

export type Platform = 'windows' | 'macos' | 'linux';

export function platformOf(userAgent: string): Platform {
	if (/Windows/i.test(userAgent)) return 'windows';
	if (/Macintosh|Mac OS X/i.test(userAgent)) return 'macos';
	return 'linux';
}

export const PLATFORM: Platform = typeof navigator === 'undefined' ? 'windows' : platformOf(navigator.userAgent);

// For styles that differ by platform (`:root[data-platform='macos']`), from the first frame.
if (typeof document !== 'undefined') document.documentElement.dataset.platform = PLATFORM;
