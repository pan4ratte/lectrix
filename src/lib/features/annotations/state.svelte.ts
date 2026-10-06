// The active annotation tool and the last-used style of each tool (section 6.5). The styles
// are a per-user convenience kept in the webview's storage; losing them only resets the
// defaults. Only styles that differ from the defaults are kept, so a tool the user never
// restyled follows the defaults of the running version.

import { changedStyles, DEFAULT_STYLES, type DrawTool, type Tool, type ToolStyle } from './tools.ts';

const STORAGE_KEY = 'lectrix.toolStyles';
/** Every tool's style, defaults included, as versions before Acrobat's defaults kept them. */
const OLD_STORAGE_KEY = 'lectrix.annotationStyles';

function loadStyles(): Record<DrawTool, ToolStyle> {
	const styles = structuredClone(DEFAULT_STYLES) as Record<DrawTool, ToolStyle>;
	try {
		localStorage.removeItem(OLD_STORAGE_KEY);
		const raw = localStorage.getItem(STORAGE_KEY);
		if (!raw) return styles;
		const saved = JSON.parse(raw) as Partial<Record<DrawTool, Partial<ToolStyle>>>;
		for (const tool of Object.keys(styles) as DrawTool[]) {
			const s = saved[tool];
			if (!s) continue;
			if (typeof s.color === 'string' && /^#[0-9a-f]{6}$/i.test(s.color)) styles[tool].color = s.color;
			if (typeof s.opacity === 'number' && s.opacity >= 0.1 && s.opacity <= 1) styles[tool].opacity = s.opacity;
			if (typeof s.width === 'number' && s.width >= 0.25 && s.width <= 48) styles[tool].width = s.width;
			if (typeof s.fontSize === 'number' && s.fontSize >= 4 && s.fontSize <= 144) styles[tool].fontSize = s.fontSize;
		}
	} catch {
		// Storage unavailable or unreadable: defaults.
	}
	return styles;
}

class ToolState {
	tool = $state<Tool>('select');
	styles = $state<Record<DrawTool, ToolStyle>>(loadStyles());
	/** Settings: styles are kept across restarts, and restyling an annotation sets its
	 * tool's style. Off, every start begins from the defaults. */
	remember = true;

	style(tool: DrawTool): ToolStyle {
		return this.styles[tool];
	}

	setStyle(tool: DrawTool, patch: Partial<ToolStyle>) {
		this.styles[tool] = { ...this.styles[tool], ...patch };
		if (this.remember) this.save();
	}

	/** Follows the setting; turning it off forgets the remembered styles. */
	setRemember(on: boolean) {
		if (on === this.remember) return;
		this.remember = on;
		if (on) {
			this.save();
			return;
		}
		this.styles = structuredClone(DEFAULT_STYLES) as Record<DrawTool, ToolStyle>;
		try {
			localStorage.removeItem(STORAGE_KEY);
		} catch {
			// Nothing stored to forget.
		}
	}

	private save() {
		try {
			localStorage.setItem(STORAGE_KEY, JSON.stringify(changedStyles(this.styles)));
		} catch {
			// Not remembered this time.
		}
	}
}

export const tools = new ToolState();
