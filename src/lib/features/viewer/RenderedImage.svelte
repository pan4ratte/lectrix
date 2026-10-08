<script lang="ts">
	// One canvas showing a rendered page or tile. It keeps showing its previous pixels
	// (stretched by CSS) until the image for a new key arrives, so zooming never blanks.
	// When the page was turned in the document meanwhile, those pixels are turned with it,
	// so they keep their proportions instead of being squeezed into the new shape.
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
		/** The page's `/Rotate` the image is rendered for (whole pages only). */
		rotation?: number;
		ondrawn?: (imageKey: string) => void;
	}

	let { imageKey, url, priority, x, y, width, height, rotation = 0, ondrawn }: Props = $props();

	let canvas: HTMLCanvasElement | undefined = $state();
	let drawnKey = '';
	let everDrawn = $state(false);
	/** The `/Rotate` the pixels on the canvas were rendered for. */
	let drawnRotation = $state(untrack(() => rotation));
	/** How far the page has turned since those pixels were rendered, clockwise. */
	const turn = $derived(everDrawn ? (((rotation - drawnRotation) % 360) + 360) % 360 : 0);
	/** Turned a quarter, the old pixels are laid out in the new box's shape on its side. */
	const sideways = $derived(turn === 90 || turn === 270);
	let failed = $state(false);
	let ticket: ReturnType<typeof scheduler.request> | null = null;

	$effect(() => {
		const key = imageKey;
		const source = url;
		if (key === drawnKey) return;
		failed = false;
		const rendered = untrack(() => rotation);
		const current = scheduler.request(key, source, untrack(() => priority));
		ticket = current;
		current.promise.then(
			(image) => {
				// A render that had started before the key changed still arrives; drawing it
				// would put the wrong pixels where the new image belongs.
				if (!canvas || ticket !== current) return;
				if (canvas.width !== image.width) canvas.width = image.width;
				if (canvas.height !== image.height) canvas.height = image.height;
				// A CPU-backed canvas: Chromium keeps an accelerated canvas's pixels in the GPU process
				// as well, which doubled the memory of every page on screen (ADR 0002).
				const ctx = canvas.getContext('2d', { willReadFrequently: /Windows/i.test(navigator.userAgent) });
				if (ctx) image.draw(ctx);
				drawnKey = key;
				drawnRotation = rendered;
				everDrawn = true;
				ondrawn?.(key);
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
	style:left="{sideways ? x + (width - height) / 2 : x}px"
	style:top="{sideways ? y + (height - width) / 2 : y}px"
	style:width="{sideways ? height : width}px"
	style:height="{sideways ? width : height}px"
	style:transform={turn ? `rotate(${turn}deg)` : undefined}
	width="1"
	height="1"
	aria-hidden="true"
></canvas>
