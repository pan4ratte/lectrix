// The active annotation tool, the colour every tool draws with, and the last-used opacity,
// stroke width and font size of each tool (section 6.5). They are a per-user convenience
// kept in the webview's storage; losing them only resets the defaults. Only what differs
// from the defaults is kept, so a tool the user never restyled follows the defaults of the
// running version.

import {
	changedStyles,
	DEFAULT_COLOR,
	DEFAULT_STYLES,
	PRESET_COLORS,
	presetColors,
	type DrawTool,
	type OwnStyle,
	type PresetColor,
	type Tool,
	type ToolStyle
} from './tools.ts';

const STORAGE_KEY = 'lectrix.toolStyles';
const COLOR_KEY = 'lectrix.toolColor';
/** Every tool's style, defaults included, as versions before Acrobat's defaults kept them. */
const OLD_STORAGE_KEY = 'lectrix.annotationStyles';

const isColor = (value: unknown): value is string => typeof value === 'string' && /^#[0-9a-f]{6}$/i.test(value);

function defaultStyles(): Record<DrawTool, OwnStyle> {
	return structuredClone(DEFAULT_STYLES) as Record<DrawTool, OwnStyle>;
}

function loadStyles(): { styles: Record<DrawTool, OwnStyle>; color: string } {
	const styles = defaultStyles();
	let color = DEFAULT_COLOR;
	try {
		localStorage.removeItem(OLD_STORAGE_KEY);
		const raw = localStorage.getItem(STORAGE_KEY);
		const saved = raw ? (JSON.parse(raw) as Partial<Record<DrawTool, Partial<ToolStyle>>>) : {};
		for (const tool of Object.keys(styles) as DrawTool[]) {
			const s = saved[tool];
			if (!s) continue;
			if (typeof s.opacity === 'number' && s.opacity >= 0.1 && s.opacity <= 1) styles[tool].opacity = s.opacity;
			if (typeof s.width === 'number' && s.width >= 0.25 && s.width <= 48) styles[tool].width = s.width;
			if (typeof s.fontSize === 'number' && s.fontSize >= 4 && s.fontSize <= 144) styles[tool].fontSize = s.fontSize;
		}
		const own = localStorage.getItem(COLOR_KEY);
		// Versions that kept a colour for each tool: the highlight's becomes everyone's.
		if (isColor(own)) color = own.toLowerCase();
		else if (isColor(saved.highlight?.color)) color = saved.highlight.color.toLowerCase();
	} catch {
		// Storage unavailable or unreadable: defaults.
	}
	return { styles, color };
}

const loaded = loadStyles();

class ToolState {
	tool = $state<Tool>('select');
	/** The colour new annotations get, whatever the tool. */
	color = $state(loaded.color);
	styles = $state<Record<DrawTool, OwnStyle>>(loaded.styles);
	/** Settings: styles are kept across restarts, and restyling an annotation sets the
	 * colour, and its tool's opacity. Off, every start begins from the defaults. */
	remember = true;
	/** The colour panel's presets, the same for every tool: Acrobat's, with the ones the
	 * user changed in Settings. */
	presets = $state.raw<readonly PresetColor[]>(PRESET_COLORS);

	/** Follows Settings' preset colours (the ones that differ from the defaults). */
	setPresets(custom: Readonly<Record<number, string>> | undefined) {
		this.presets = presetColors(custom);
	}

	style(tool: DrawTool): ToolStyle {
		return { ...this.styles[tool], color: this.color };
	}

	/** A colour goes to every tool; the rest to `tool` alone. */
	setStyle(tool: DrawTool, patch: Partial<ToolStyle>) {
		const { color, ...own } = patch;
		if (color !== undefined) this.color = color;
		if (Object.keys(own).length) this.styles[tool] = { ...this.styles[tool], ...own };
		if (this.remember) this.save();
	}

	setColor(color: string) {
		this.color = color;
		if (this.remember) this.save();
	}

	/** The `n`th preset (from 1) as the colour, as its number key picks it. */
	pickPreset(n: number) {
		const preset = this.presets[n - 1];
		if (preset) this.setColor(preset.value);
	}

	/** Follows the setting; turning it off forgets the remembered styles. */
	setRemember(on: boolean) {
		if (on === this.remember) return;
		this.remember = on;
		if (on) {
			this.save();
			return;
		}
		this.styles = defaultStyles();
		this.color = DEFAULT_COLOR;
		try {
			localStorage.removeItem(STORAGE_KEY);
			localStorage.removeItem(COLOR_KEY);
		} catch {
			// Nothing stored to forget.
		}
	}

	private save() {
		try {
			localStorage.setItem(STORAGE_KEY, JSON.stringify(changedStyles(this.styles)));
			if (this.color === DEFAULT_COLOR) localStorage.removeItem(COLOR_KEY);
			else localStorage.setItem(COLOR_KEY, this.color);
		} catch {
			// Not remembered this time.
		}
	}
}

export const tools = new ToolState();
