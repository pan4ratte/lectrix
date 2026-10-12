// Page-image requests, scheduled so the pages the user is looking at come first.
//
// The Rust side renders on a small worker pool and cannot see that a page scrolled away,
// so ordering and dropping stale requests happens here: at most `maxInFlight` requests
// run at once, the rest wait in a queue ordered by priority (lower first), and a request
// cancelled before it starts is never sent.

import { fetchPageImage, warmPageImage, type PageImage } from '#lib/ipc/index.ts';

interface Job<T = unknown> {
	key: string;
	/** Sends the request. */
	start: () => Promise<T>;
	priority: number;
	seq: number;
	waiters: number;
	promise: Promise<T>;
	resolve: (result: T) => void;
	reject: (error: unknown) => void;
	started: boolean;
}

/** Priorities from here on are background work (thumbnails), behind every page on screen. */
export const BACKGROUND_PRIORITY = 1_000_000;

/**
 * Pages rendered ahead are background work too, taken with the thumbnails nearest the
 * current page (a thumbnail's priority is this plus its distance from that page): behind
 * a panel's worth of thumbnails, the next page would often not be ready in time.
 */
export const AHEAD_PRIORITY = BACKGROUND_PRIORITY;

export class CancelledError extends Error {
	constructor() {
		super('cancelled');
	}
}

export interface Ticket<T = PageImage> {
	promise: Promise<T>;
	/** Lowers interest in the image; the request is dropped if nobody else wants it. */
	cancel(): void;
	/** Re-prioritizes the request (if it has not started yet). */
	setPriority(priority: number): void;
}

export class RenderScheduler {
	private queue = new Map<string, Job>();
	private inFlight = 0;
	private backgroundInFlight = 0;
	private seq = 0;

	constructor(private readonly maxInFlight = 3) {}

	request(key: string, url: string, priority: number): Ticket {
		return this.enqueue(key, priority, () => fetchPageImage(url));
	}

	/**
	 * Has the image at `url` rendered and kept in Rust, without fetching it: a page rendered
	 * ahead. The page's own request, once it is shown, is then answered from that.
	 */
	warm(url: string, priority: number): Ticket<void> {
		return this.enqueue(`warm ${url}`, priority, () => warmPageImage(url));
	}

	private enqueue<T>(key: string, priority: number, start: () => Promise<T>): Ticket<T> {
		let job = this.queue.get(key) as Job<T> | undefined;
		if (!job) {
			let resolve!: (result: T) => void;
			let reject!: (error: unknown) => void;
			const promise = new Promise<T>((res, rej) => {
				resolve = res;
				reject = rej;
			});
			job = {
				key,
				start,
				priority,
				seq: this.seq++,
				waiters: 0,
				promise,
				resolve,
				reject,
				started: false
			};
			this.queue.set(key, job as Job);
		} else {
			job.priority = Math.min(job.priority, priority);
		}
		job.waiters++;
		const current = job;
		let cancelled = false;
		queueMicrotask(() => this.pump());
		return {
			promise: current.promise,
			cancel: () => {
				if (cancelled) return;
				cancelled = true;
				current.waiters--;
				if (current.waiters <= 0 && !current.started) {
					this.queue.delete(current.key);
					current.reject(new CancelledError());
				}
			},
			setPriority: (priority: number) => {
				if (!current.started) current.priority = priority;
			}
		};
	}

	/** Number of requests waiting to start (for tests and diagnostics). */
	get pending(): number {
		let n = 0;
		for (const job of this.queue.values()) if (!job.started) n++;
		return n;
	}

	private next(): Job | null {
		let best: Job | null = null;
		for (const job of this.queue.values()) {
			if (job.started) continue;
			if (!best || job.priority < best.priority || (job.priority === best.priority && job.seq < best.seq)) {
				best = job;
			}
		}
		return best;
	}

	private pump() {
		while (this.inFlight < this.maxInFlight) {
			const job = this.next();
			if (!job) return;
			// Thumbnails and pages rendered ahead wait while pages on screen render, and leave
			// a slot free for them: MuPDF decodes one JPEG 2000 image at a time across the
			// whole app, and a page holding a large scan takes about a second, so a thumbnail
			// decoding first would keep the page waiting that long.
			const background = job.priority >= BACKGROUND_PRIORITY;
			const foregroundInFlight = this.inFlight - this.backgroundInFlight;
			if (background && (foregroundInFlight > 0 || this.backgroundInFlight >= this.maxInFlight - 1)) return;
			job.started = true;
			this.inFlight++;
			if (background) this.backgroundInFlight++;
			job.start()
				.then(job.resolve, job.reject)
				.finally(() => {
					this.inFlight--;
					if (background) this.backgroundInFlight--;
					this.queue.delete(job.key);
					this.pump();
				});
		}
	}
}

/** One scheduler for the whole app: all documents share the renderer. */
export const scheduler = new RenderScheduler(3);

/** Ignores the rejection of a cancelled request; reports anything else. */
export function isCancelled(error: unknown): boolean {
	return error instanceof CancelledError;
}
