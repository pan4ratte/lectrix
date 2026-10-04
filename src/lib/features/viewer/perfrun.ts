// Scripted performance measurements, run when the app starts with LECTRIX_PERF set
// (tests/perf/measure.ps1). Results are printed as [lectrix-metric] lines by Rust.

import { logMetric, pageUrl } from '#lib/ipc/index.ts';
import type { DocTab } from '#lib/stores/doc.svelte.ts';

import { blankTracker } from './perf.ts';
import { renderScale } from './zoom.ts';

let firstPaint: (() => void) | null = null;
const firstPaintDone = new Promise<void>((resolve) => (firstPaint = resolve));

/** Called whenever a page image is drawn; the first call marks the first visible page. */
export function markPageDrawn() {
	firstPaint?.();
	firstPaint = null;
}

export function firstPagePainted(): Promise<void> {
	return firstPaintDone;
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const mean = (values: number[]) => values.reduce((a, b) => a + b, 0) / Math.max(1, values.length);

function serverMs(header: string | null, keys: string[]): number {
	if (!header) return NaN;
	const parts = Object.fromEntries(header.split(';').map((p) => p.split('=') as [string, string]));
	return keys.reduce((sum, k) => sum + Number(parts[k] ?? NaN), 0);
}

/**
 * Raw RGBA into a canvas versus PNG decoded by the browser, end to end (request to pixels
 * on a canvas), on the same pages at the current zoom (Phase 0 review, decision 3).
 */
async function compareFormats(tab: DocTab, samples = 16) {
	const dpr = window.devicePixelRatio || 1;
	// A scale off the ladder, so the RGBA image cache cannot answer from memory.
	const scale = renderScale(tab.zoom, dpr) + 0.001;
	const canvas = document.createElement('canvas');
	const ctx = canvas.getContext('2d');
	if (!ctx) return;
	const rgba: number[] = [];
	const rgbaServer: number[] = [];
	const rgbaBytes: number[] = [];
	const png: number[] = [];
	const pngServer: number[] = [];
	const pngBytes: number[] = [];
	const count = tab.pageCount;
	for (let i = 0; i < samples; i++) {
		const page = Math.floor((i * count) / samples);
		// Alternate which format goes first, so neither always gets a warm display list.
		const order = i % 2 === 0 ? ['rgba', 'png'] : ['png', 'rgba'];
		for (const format of order) {
			const url = pageUrl(tab.id, page, scale, tab.state.revision, undefined, format as 'rgba' | 'png');
			const t0 = performance.now();
			if (format === 'rgba') {
				const response = await fetch(url);
				const timing = response.headers.get('X-Lectrix-Timing');
				const width = Number(response.headers.get('X-Lectrix-Width'));
				const height = Number(response.headers.get('X-Lectrix-Height'));
				const buffer = await response.arrayBuffer();
				canvas.width = width;
				canvas.height = height;
				ctx.putImageData(new ImageData(new Uint8ClampedArray(buffer), width, height), 0, 0);
				rgba.push(performance.now() - t0);
				rgbaServer.push(serverMs(timing, ['render']));
				rgbaBytes.push(buffer.byteLength);
			} else {
				const response = await fetch(url);
				const timing = response.headers.get('X-Lectrix-Timing');
				const blob = await response.blob();
				const bitmap = await createImageBitmap(blob);
				canvas.width = bitmap.width;
				canvas.height = bitmap.height;
				ctx.drawImage(bitmap, 0, 0);
				bitmap.close();
				png.push(performance.now() - t0);
				pngServer.push(serverMs(timing, ['render', 'encode']));
				pngBytes.push(blob.size);
			}
		}
	}
	await logMetric('format_scale_x1000', scale * 1000);
	await logMetric('rgba_end_to_end_mean_ms', mean(rgba));
	await logMetric('rgba_server_mean_ms', mean(rgbaServer));
	await logMetric('rgba_kib_mean', mean(rgbaBytes) / 1024);
	await logMetric('png_end_to_end_mean_ms', mean(png));
	await logMetric('png_server_mean_ms', mean(pngServer));
	await logMetric('png_kib_mean', mean(pngBytes) / 1024);
}

function pageHeightPx(): number {
	const page = document.querySelector<HTMLElement>('.viewer-scroll .page');
	return page ? page.offsetHeight + 12 : 1000;
}

async function report(name: string, extra: Record<string, number>) {
	const result = blankTracker.stop();
	for (const [key, value] of Object.entries(extra)) await logMetric(`${name}_${key}`, value);
	// Times a page was on screen without pixels, and for how long.
	await logMetric(`${name}_blank_events`, result.count);
	await logMetric(`${name}_blank_max_ms`, result.max);
	await logMetric(`${name}_blank_p95_ms`, result.p95);
	await logMetric(`${name}_blank_median_ms`, result.median);
	await logMetric(`${name}_blank_over_200ms`, result.over200);
}

/** Scrolls at a steady speed and reports how long pages stayed blank while visible. */
async function scrollTest(name: string, pxPerSecond: number, maxMs: number) {
	const scroller = document.querySelector<HTMLElement>('.viewer-scroll');
	if (!scroller) return;
	scroller.scrollTop = 0;
	await sleep(1500);
	blankTracker.start();
	const start = performance.now();
	await new Promise<void>((resolve) => {
		const step = () => {
			const elapsed = performance.now() - start;
			scroller.scrollTop = (elapsed / 1000) * pxPerSecond;
			const atEnd = scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - 1;
			if (elapsed >= maxMs || atEnd) resolve();
			else requestAnimationFrame(step);
		};
		requestAnimationFrame(step);
	});
	const seconds = (performance.now() - start) / 1000;
	const passed = scroller.scrollTop / pageHeightPx();
	await sleep(300);
	await report(name, { px_per_s: pxPerSecond, seconds, pages_passed: Math.round(passed) });
}

/** Jumps to random places, as when dragging the scrollbar thumb, holding each for 400 ms. */
async function jumpTest(jumps: number) {
	const scroller = document.querySelector<HTMLElement>('.viewer-scroll');
	if (!scroller) return;
	blankTracker.start();
	let seed = 7;
	for (let i = 0; i < jumps; i++) {
		seed = (seed * 48271) % 2147483647;
		scroller.scrollTop = (seed / 2147483647) * (scroller.scrollHeight - scroller.clientHeight);
		await sleep(400);
	}
	await report('scroll_jump', { jumps });
}

/** The renderer's JavaScript heap and page surfaces, for memory investigations. */
async function memoryMetrics(name: string) {
	const memory = (performance as Performance & { memory?: { usedJSHeapSize: number } }).memory;
	if (memory) await logMetric(`${name}_js_heap_mb`, memory.usedJSHeapSize / 2 ** 20);
	await logMetric(`${name}_canvases`, document.querySelectorAll('canvas').length);
	await logMetric(`${name}_images`, document.querySelectorAll('img').length);
}

export async function runPerf(tab: DocTab, scrollOnly = false) {
	await sleep(1000);
	await memoryMetrics('before_scroll');
	await logMetric('image_format_png', pageUrl(0, 0, 1, 0).includes('fmt=png') ? 1 : 0);
	if (!scrollOnly) await compareFormats(tab);
	await scrollTest('scroll_steady', 2000, 30_000);
	await scrollTest('scroll_fast', 6000, 30_000);
	await jumpTest(20);
	await sleep(1000);
	await memoryMetrics('after_scroll');
	await logMetric('perf_done', 1);
}
