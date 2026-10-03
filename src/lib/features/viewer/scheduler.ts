// Page-image requests, scheduled so the pages the user is looking at come first.
//
// The Rust side renders on a small worker pool and cannot see that a page scrolled away,
// so ordering and dropping stale requests happens here: at most `maxInFlight` requests
// run at once, the rest wait in a queue ordered by priority (lower first), and a request
// cancelled before it starts is never sent.

import { fetchPageImage, type PageImage } from '#lib/ipc/index.ts';

interface Job {
	key: string;
	url: string;
	priority: number;
	seq: number;
	waiters: number;
	promise: Promise<PageImage>;
	resolve: (image: PageImage) => void;
	reject: (error: unknown) => void;
	started: boolean;
}

export class CancelledError extends Error {
	constructor() {
		super('cancelled');
	}
}

export interface Ticket {
	promise: Promise<PageImage>;
	/** Lowers interest in the image; the request is dropped if nobody else wants it. */
	cancel(): void;
	/** Re-prioritizes the request (if it has not started yet). */
	setPriority(priority: number): void;
}

export class RenderScheduler {
	private queue = new Map<string, Job>();
	private inFlight = 0;
	private seq = 0;

	constructor(private readonly maxInFlight = 3) {}

	request(key: string, url: string, priority: number): Ticket {
		let job = this.queue.get(key);
		if (!job) {
			let resolve!: (image: PageImage) => void;
			let reject!: (error: unknown) => void;
			const promise = new Promise<PageImage>((res, rej) => {
				resolve = res;
				reject = rej;
			});
			job = {
				key,
				url,
				priority,
				seq: this.seq++,
				waiters: 0,
				promise,
				resolve,
				reject,
				started: false
			};
			this.queue.set(key, job);
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
			job.started = true;
			this.inFlight++;
			fetchPageImage(job.url)
				.then(job.resolve, job.reject)
				.finally(() => {
					this.inFlight--;
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
