import { beforeEach, describe, expect, it, vi } from 'vitest';

const ran: string[] = [];

vi.mock('#lib/commands.ts', () => ({
	commands: new Proxy({}, { get: (_, name: string) => () => ran.push(name) }),
	setViewMode: (mode: string) => ran.push(`mode ${mode}`)
}));
vi.mock('#lib/features/update/update.svelte.ts', () => ({ update: { stage: { kind: 'idle' } } }));
vi.mock('#lib/features/debug/menu.ts', () => ({ debugMenu: () => ({ label: 'Debug', items: [] }) }));
vi.mock('#lib/stores/app.svelte.ts', () => ({
	app: {
		active: null,
		recent: [{ index: 0, name: 'report.pdf', folder: 'C:\\Docs', exists: true }],
		tabs: [],
		combineActive: false,
		combine: null,
		panels: { left: ['pages'], right: [] },
		isOpen: () => false,
		isOnlyTab: () => true,
		startup: null,
		settings: null,
		openRecent: (index: number) => ran.push(`recent ${index}`)
	}
}));

const { appMenus, nativeMenus, runMenuItem, titleCase } = await import('./menus.svelte.ts');
type Native = ReturnType<typeof nativeMenus>[number];

function flat(entries: readonly Native[]): Native[] {
	return entries.flatMap((e) => (e.kind === 'submenu' ? [e, ...flat(e.items)] : [e]));
}

beforeEach(() => {
	ran.length = 0;
});

describe('the menus', () => {
	it('keep the title bar’s menus on Windows and Linux', () => {
		for (const platform of ['windows', 'linux'] as const) {
			expect(appMenus(platform).map((m) => m.label)).toEqual(['File', 'Edit', 'View', 'Document', 'Help']);
		}
	});

	it('are arranged as Mac apps arrange theirs on macOS', () => {
		const menus = appMenus('macos');
		expect(menus.map((m) => m.label)).toEqual(['Lectrix', 'File', 'Edit', 'View', 'Document', 'Window', 'Help']);
		const items = flat(nativeMenus(menus, false));
		const ids = items.flatMap((e) => (e.kind === 'item' ? [e.id] : []));
		// Each item is named once, so a choice runs one thing.
		expect(new Set(ids).size).toBe(ids.length);
		// Settings, About and Quit are in the application menu, and the text field's editing
		// is the system's.
		const appItems = nativeMenus(menus, false)[0]!;
		expect(appItems.kind === 'submenu' && appItems.items.map((e) => (e.kind === 'item' ? e.label : e.kind === 'system' ? e.item : '-'))).toEqual([
			'About Lectrix',
			'Check for Updates…',
			'-',
			'Settings…',
			'-',
			'services',
			'-',
			'hide',
			'hideOthers',
			'showAll',
			'-',
			'Quit Lectrix'
		]);
		expect(items.some((e) => e.kind === 'system' && e.item === 'copy')).toBe(true);
		expect(items.find((e) => e.kind === 'item' && e.id === 'settings')).toMatchObject({ accelerator: 'Cmd+,' });
		expect(items.find((e) => e.kind === 'item' && e.id === 'exit')).toMatchObject({ accelerator: 'Cmd+Q' });
		expect(items.find((e) => e.kind === 'item' && e.id === 'saveAs')).toMatchObject({ label: 'Save As…', accelerator: 'Cmd+Shift+S' });
		// A file's name stays as it is.
		expect(items.find((e) => e.kind === 'item' && e.id === 'recent:0')).toMatchObject({ label: 'report.pdf' });
	});

	it('offer nothing that acts on the window while a dialog is open', () => {
		const items = flat(nativeMenus(appMenus('macos'), true));
		expect(items.filter((e) => e.kind === 'item' && e.enabled)).toEqual([]);
		expect(items.some((e) => e.kind === 'system')).toBe(true);
	});

	it('run the item a choice in the menu bar names', () => {
		const menus = appMenus('macos');
		expect(runMenuItem(menus, 'open')).toBe(true);
		expect(runMenuItem(menus, 'recent:0')).toBe(true);
		expect(runMenuItem(menus, 'viewMode:twoPage')).toBe(true);
		expect(runMenuItem(menus, 'nothing')).toBe(false);
		expect(ran).toEqual(['open', 'recent 0', 'mode twoPage']);
	});
});

describe('titleCase', () => {
	it('writes labels as Mac menus do', () => {
		expect(titleCase('Save as (optimized)…')).toBe('Save As (Optimized)…');
		expect(titleCase('Insert pages from file…')).toBe('Insert Pages from File…');
		expect(titleCase('Rotate view counter-clockwise')).toBe('Rotate View Counter-Clockwise');
		expect(titleCase('Go to page…')).toBe('Go to Page…');
		expect(titleCase('What’s new')).toBe('What’s New');
		expect(titleCase('Undo add bookmark')).toBe('Undo Add Bookmark');
	});
});
