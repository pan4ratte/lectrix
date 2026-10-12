import { describe, expect, it, vi } from 'vitest';

const started: string[] = [];
const finish = new Map<string, () => void>();

vi.mock('#lib/ipc/index.ts', () => ({
	fetchPageImage: (url: string) => {
		started.push(url);
		return new Promise((resolve) => finish.set(url, () => resolve({ url })));
	},
	warmPageImage: (url: string) => {
		started.push(`warm ${url}`);
		return new Promise<void>((resolve) => finish.set(`warm ${url}`, resolve));
	}
}));

const { AHEAD_PRIORITY, BACKGROUND_PRIORITY, RenderScheduler } = await import('./scheduler.ts');

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

	it('renders pages ahead behind everything on screen, and not at all once cancelled', async () => {
		started.length = 0;
		const scheduler = new RenderScheduler(3);
		// Thumbnails far from the current page, its own, and the pages next to it.
		for (let i = 5; i < 8; i++) scheduler.request(`t${i}`, `thumb-${i}`, BACKGROUND_PRIORITY + i);
		scheduler.request('t0', 'thumb-0', BACKGROUND_PRIORITY);
		const next = scheduler.warm('page-3', AHEAD_PRIORITY);
		const previous = scheduler.warm('page-1', AHEAD_PRIORITY + 1);
		scheduler.request('p', 'page-2', 0);
		await settle();
		// The page on screen alone: background work waits for it.
		expect(started).toEqual(['page-2']);
		finish.get('page-2')!();
		await settle();
		// Then the current page's thumbnail and the page after it, before other thumbnails.
		expect(started).toEqual(['page-2', 'thumb-0', 'warm page-3']);

		// The view moved on before the other started: it is never sent.
		const dropped = previous.promise.catch((error: unknown) => error);
		previous.cancel();
		finish.get('thumb-0')!();
		finish.get('warm page-3')!();
		await settle();
		expect(started).toEqual(['page-2', 'thumb-0', 'warm page-3', 'thumb-5', 'thumb-6']);
		expect(await dropped).toBeInstanceOf(Error);
		await expect(next.promise).resolves.toBeUndefined();
		for (const url of ['thumb-5', 'thumb-6']) finish.get(url)!();
		await settle();
		finish.get('thumb-7')!();
		await settle();
		expect(scheduler.pending).toBe(0);

		// A page asked for while it is rendered ahead is a request of its own: Rust has it
		// wait for that render.
		scheduler.warm('page-4', AHEAD_PRIORITY);
		await settle();
		scheduler.request('p4', 'page-4', 0);
		await settle();
		expect(started.slice(-2)).toEqual(['warm page-4', 'page-4']);
	});
});
