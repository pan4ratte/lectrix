// Measures how long pages stay blank while visible (section 2: no blank page visible for
// more than 200 ms while scrolling). Each page view reports when it becomes visible
// without pixels and when its first pixels are drawn.

interface Sample {
	ms: number;
	page: number;
}

class BlankTracker {
	private since = new Map<string, { at: number; page: number }>();
	private samples: Sample[] = [];
	recording = false;

	/** A page is on screen and has nothing drawn yet. */
	blank(key: string, page: number) {
		if (!this.recording || this.since.has(key)) return;
		this.since.set(key, { at: performance.now(), page });
	}

	/** The page got pixels, or left the screen. */
	settled(key: string) {
		const start = this.since.get(key);
		if (!start) return;
		this.since.delete(key);
		this.samples.push({ ms: performance.now() - start.at, page: start.page });
	}

	start() {
		this.samples = [];
		this.since.clear();
		this.recording = true;
	}

	/** Stops recording and summarizes (pages still blank count as blank until now). */
	stop() {
		for (const key of [...this.since.keys()]) this.settled(key);
		this.recording = false;
		const ms = this.samples.map((s) => s.ms).sort((a, b) => a - b);
		const at = (q: number) => (ms.length ? ms[Math.min(ms.length - 1, Math.floor(q * ms.length))]! : 0);
		return {
			count: ms.length,
			max: ms.length ? ms[ms.length - 1]! : 0,
			p95: at(0.95),
			median: at(0.5),
			over200: ms.filter((v) => v > 200).length
		};
	}
}

export const blankTracker = new BlankTracker();
