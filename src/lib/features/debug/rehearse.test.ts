import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { UpdateStage } from '#lib/features/update/update.svelte.ts';

const update = vi.hoisted(() => ({
	stage: { kind: 'idle' } as { kind: string },
	simulate: vi.fn<(stage: UpdateStage, installed?: boolean) => void>(),
	notNow: vi.fn()
}));
vi.mock('#lib/features/update/update.svelte.ts', () => ({ update, notifyUpToDate: vi.fn() }));
vi.mock('#lib/stores/app.svelte.ts', () => ({ app: { tabs: [], notify: vi.fn(), showError: vi.fn() } }));
vi.mock('@tauri-apps/api/app', () => ({ getVersion: async () => '1.0.0' }));

const { changesSince } = await import('#lib/features/changelog/changelog.ts');
const { nextVersion, rehearsedUpdate, rehearseUpdate } = await import('./rehearse.ts');

const CHANGELOG = `# Changelog

## Unreleased

### General

- Something new.

## 1.0.0

### General

- First release.
`;

beforeEach(() => vi.clearAllMocks());

describe('rehearsed update', () => {
	it('is the next minor version', () => {
		expect(nextVersion('1.0.0')).toBe('1.1.0');
		expect(nextVersion('1.2.3-beta.1')).toBe('1.3.0');
	});

	it('brings what Unreleased lists', () => {
		const info = rehearsedUpdate(CHANGELOG, '1.0.0');
		expect(info.version).toBe('1.1.0');
		expect(info.currentVersion).toBe('1.0.0');
		const shown = changesSince(info.notes, info.currentVersion);
		expect(shown.map((r) => r.version)).toEqual(['1.1.0']);
		expect(shown[0]!.sections[0]!.items[0]![0]!.text).toBe('Something new.');
	});

	it('without anything unreleased, brings the newest version', () => {
		const released = CHANGELOG.replace('### General\n\n- Something new.\n', '').replace(
			'## 1.0.0',
			'## 1.1.0\n\n### General\n\n- Newer.\n\n## 1.0.0'
		);
		const info = rehearsedUpdate(released, '1.1.0');
		expect(info).toMatchObject({ version: '1.1.0', currentVersion: '1.0.0' });
		expect(changesSince(info.notes, info.currentVersion).map((r) => r.version)).toEqual(['1.1.0']);
	});

	it('shows the notice with the real changelog', async () => {
		await rehearseUpdate('available');
		const stage = update.simulate.mock.calls[0]![0];
		expect(stage.kind).toBe('available');
		if (stage.kind === 'available') expect(stage.info.currentVersion).toBe('1.0.0');
	});
});
