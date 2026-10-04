import { describe, expect, it, vi } from 'vitest';

const started: string[] = [];
const finish = new Map<string, () => void>();

vi.mock('#lib/ipc/index.ts', () => ({
	fetchPageImage: (url: string) => {
		started.push(url);
		return new Promise((resolve) => finish.set(url, () => resolve({ url })));
	}
}));

const { BACKGROUND_PRIORITY, RenderScheduler } = await import('./scheduler.ts');

const settle = () => new Promise((r) => setTimeout(r, 0));

describe('RenderScheduler', () => {
	it('keeps a slot free for pages on screen, and holds thumbnails while pages render', async () => {
		started.length = 0;
		const scheduler = new RenderScheduler(3);
		for (let i = 0; i < 5; i++) scheduler.request(`t${i}`, `thumb-${i}`, BACKGROUND_PRIORITY + i);
		await settle();
		expect(started).toEqual(['thumb-0', 'thumb-1']);

		// The first page asks later, and starts at once.
		scheduler.request('p0', 'page-0', 0);
		await settle();
		expect(started).toEqual(['thumb-0', 'thumb-1', 'page-0']);

		// While the page renders, a finished thumbnail is not replaced.
		finish.get('thumb-0')!();
		await settle();
		expect(started).toEqual(['thumb-0', 'thumb-1', 'page-0']);

		// Once it is done, thumbnails go on, still leaving a slot free.
		finish.get('page-0')!();
		await settle();
		expect(started).toEqual(['thumb-0', 'thumb-1', 'page-0', 'thumb-2']);
		finish.get('thumb-1')!();
		await settle();
		expect(started).toEqual(['thumb-0', 'thumb-1', 'page-0', 'thumb-2', 'thumb-3']);
	});
});
