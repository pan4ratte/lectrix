<script lang="ts">
	// One canvas showing a rendered page or tile. It keeps showing its previous pixels
	// (stretched by CSS) until the image for a new key arrives, so zooming never blanks.
	import { untrack } from 'svelte';

	import { logError } from '#lib/ipc/index.ts';

	import { isCancelled, scheduler } from './scheduler.ts';

	interface Props {
		/** Identifies the image: a new key means new pixels are needed. */
		imageKey: string;
		url: string;
		priority: number;
		/** Placement in CSS pixels inside the page. */
		x: number;
		y: number;
		width: number;
		height: number;
		ondrawn?: () => void;
	}

	let { imageKey, url, priority, x, y, width, height, ondrawn }: Props = $props();

	let canvas: HTMLCanvasElement | undefined = $state();
	let drawnKey = '';
	let everDrawn = $state(false);
	let failed = $state(false);
	let ticket: ReturnType<typeof scheduler.request> | null = null;

	$effect(() => {
		const key = imageKey;
		const source = url;
		if (key === drawnKey) return;
		failed = false;
		const current = scheduler.request(key, source, untrack(() => priority));
		ticket = current;
		current.promise.then(
			(image) => {
				if (!canvas) return;
				if (canvas.width !== image.width) canvas.width = image.width;
				if (canvas.height !== image.height) canvas.height = image.height;
				// A CPU-backed canvas: Chromium keeps an accelerated canvas's pixels in the GPU process
				// as well, which doubled the memory of every page on screen (ADR 0002).
				const ctx = canvas.getContext('2d', { willReadFrequently: true });
				if (ctx) image.draw(ctx);
				drawnKey = key;
				everDrawn = true;
				ondrawn?.();
			},
			(error: unknown) => {
				if (isCancelled(error)) return;
				failed = true;
				void logError(`page image ${source}: ${String(error)}`);
			}
		);
		return () => {
			current.cancel();
			if (ticket === current) ticket = null;
		};
	});

	$effect(() => {
		ticket?.setPriority(priority);
	});

	// Free the canvas's pixels as soon as it leaves the page, instead of whenever the
	// garbage collector gets to the detached element (section 2 memory targets).
	$effect(() => {
		const element = canvas;
		return () => {
			if (!element) return;
			element.width = 0;
			element.height = 0;
		};
	});
</script>

<canvas
	bind:this={canvas}
	class="absolute block"
	class:opacity-0={failed && !everDrawn}
	style:left="{x}px"
	style:top="{y}px"
	style:width="{width}px"
	style:height="{height}px"
	width="1"
	height="1"
	aria-hidden="true"
></canvas>
