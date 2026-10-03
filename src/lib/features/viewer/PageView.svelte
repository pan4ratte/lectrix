<script lang="ts">
	// One page: its rendered pixels (whole, or tiles at high zoom), and overlays for the
	// text selection and search hits. Inside, everything is laid out unrotated in page
	// points scaled to CSS pixels; a CSS transform applies the view rotation.
	import { pageUrl, type PageSize } from '#lib/ipc/index.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { CSS_PX_PER_PT, type Rotation } from './layout.ts';
	import { blankTracker } from './perf.ts';
	import { markPageDrawn } from './perfrun.ts';
	import RenderedImage from './RenderedImage.svelte';
	import { ordered, selectionRects } from './selection.ts';
	import {
		TILE_SIZE,
		TILE_THRESHOLD_PIXELS,
		UNDERLAY_PIXELS,
		pixelSize,
		renderScale,
		scaleForPixels
	} from './zoom.ts';

	interface Props {
		tab: DocTab;
		index: number;
		size: PageSize;
		/** Top-left corner and on-screen size of the (rotated) page, CSS pixels. */
		left: number;
		top: number;
		width: number;
		height: number;
		zoom: number;
		rotation: Rotation;
		devicePixelRatio: number;
		/** The part of the page box that is on screen (plus a margin), CSS pixels relative
		 * to the page box; null when the page is off screen. */
		visibleRect: { x0: number; y0: number; x1: number; y1: number } | null;
		/** Whether any of the page is in the viewport itself (for blank tracking). */
		onscreen: boolean;
		priority: number;
	}

	let {
		tab,
		index,
		size,
		left,
		top,
		width,
		height,
		zoom,
		rotation,
		devicePixelRatio,
		visibleRect,
		onscreen,
		priority
	}: Props = $props();

	const k = $derived(zoom * CSS_PX_PER_PT);
	/** Unrotated page size in CSS pixels. */
	const innerW = $derived(size.width * k);
	const innerH = $derived(size.height * k);
	const transform = $derived(
		rotation === 90
			? `translate(${innerH}px, 0) rotate(90deg)`
			: rotation === 180
				? `translate(${innerW}px, ${innerH}px) rotate(180deg)`
				: rotation === 270
					? `translate(0, ${innerW}px) rotate(270deg)`
					: 'none'
	);

	const revision = $derived(tab.state.revision);
	const scale = $derived(renderScale(zoom, devicePixelRatio));
	const pixels = $derived(pixelSize(size, scale));
	const tiled = $derived(pixels.width * pixels.height > TILE_THRESHOLD_PIXELS);
	const underlayScale = $derived(
		Math.min(scale, Math.round(scaleForPixels(size, UNDERLAY_PIXELS) * 1000) / 1000)
	);

	/** Maps a rectangle in the rotated page box (CSS px) to unrotated CSS px. */
	function unrotate(r: { x0: number; y0: number; x1: number; y1: number }) {
		const pts: [number, number][] = [
			[r.x0, r.y0],
			[r.x1, r.y1]
		];
		const mapped = pts.map(([dx, dy]) => {
			switch (rotation) {
				case 90:
					return [dy, innerH - dx];
				case 180:
					return [innerW - dx, innerH - dy];
				case 270:
					return [innerW - dy, dx];
				default:
					return [dx, dy];
			}
		});
		const xs = mapped.map((p) => p[0]!);
		const ys = mapped.map((p) => p[1]!);
		return { x0: Math.min(...xs), y0: Math.min(...ys), x1: Math.max(...xs), y1: Math.max(...ys) };
	}

	/** Tiles intersecting the visible part of the page, in device pixels at `scale`. */
	const tiles = $derived.by(() => {
		if (!tiled || !visibleRect) return [];
		const r = unrotate(visibleRect);
		const f = scale / k; // device pixels per CSS pixel
		const tx0 = Math.max(0, Math.floor((r.x0 * f) / TILE_SIZE));
		const ty0 = Math.max(0, Math.floor((r.y0 * f) / TILE_SIZE));
		const tx1 = Math.min(Math.ceil(pixels.width / TILE_SIZE), Math.ceil((r.x1 * f) / TILE_SIZE));
		const ty1 = Math.min(Math.ceil(pixels.height / TILE_SIZE), Math.ceil((r.y1 * f) / TILE_SIZE));
		const out = [];
		for (let ty = ty0; ty < ty1; ty++) {
			for (let tx = tx0; tx < tx1; tx++) {
				const x = tx * TILE_SIZE;
				const y = ty * TILE_SIZE;
				const w = Math.min(TILE_SIZE, pixels.width - x);
				const h = Math.min(TILE_SIZE, pixels.height - y);
				out.push({ key: `${tx},${ty}`, x, y, w, h });
			}
		}
		return out;
	});

	// Pixels on screen for the current revision (the whole page, or the tile underlay).
	let drawnRevision = $state(-1);
	const hasPixels = $derived(drawnRevision === revision);
	const trackKey = $derived(`${tab.id}:${index}`);

	$effect(() => {
		const key = trackKey;
		if (onscreen && !hasPixels) blankTracker.blank(key, index);
		else blankTracker.settled(key);
	});
	$effect(() => {
		const key = trackKey;
		return () => blankTracker.settled(key);
	});

	// Load text geometry for selection as soon as the page is near the screen.
	$effect(() => {
		void tab.state.revision;
		void tab.loadText(index);
	});

	const selection = $derived.by(() => {
		void tab.textVersion;
		const sel = tab.selection;
		const text = tab.text(index);
		if (!sel || !text) return [];
		const [start, end] = ordered(sel.anchor, sel.focus);
		if (index < start.page || index > end.page) return [];
		return selectionRects(text, start, end);
	});

	const hits = $derived(tab.search.byPage.get(index) ?? []);
	const label = $derived(tab.labels?.[index]);
</script>

<div
	class="page absolute overflow-hidden bg-white shadow-[0_1px_3px_var(--color-page-shadow)]"
	style:left="{left}px"
	style:top="{top}px"
	style:width="{width}px"
	style:height="{height}px"
	data-page={index}
	role="img"
	aria-label={label ? `Page ${label} (${index + 1} of ${tab.pageCount})` : `Page ${index + 1} of ${tab.pageCount}`}
>
	<div
		class="absolute top-0 left-0 origin-top-left"
		style:width="{innerW}px"
		style:height="{innerH}px"
		style:transform
	>
		{#if tiled}
			<RenderedImage
				imageKey="{tab.id}:{index}:{revision}:{underlayScale}"
				url={pageUrl(tab.id, index, underlayScale, revision)}
				{priority}
				x={0}
				y={0}
				width={innerW}
				height={innerH}
				ondrawn={() => {
					drawnRevision = revision;
					markPageDrawn();
				}}
			/>
			{#each tiles as t (t.key)}
				<RenderedImage
					imageKey="{tab.id}:{index}:{revision}:{scale}:{t.key}"
					url={pageUrl(tab.id, index, scale, revision, { x: t.x, y: t.y, width: t.w, height: t.h })}
					priority={priority + 0.5}
					x={(t.x * k) / scale}
					y={(t.y * k) / scale}
					width={(t.w * k) / scale}
					height={(t.h * k) / scale}
				/>
			{/each}
		{:else}
			<RenderedImage
				imageKey="{tab.id}:{index}:{revision}:{scale}"
				url={pageUrl(tab.id, index, scale, revision)}
				{priority}
				x={0}
				y={0}
				width={innerW}
				height={innerH}
				ondrawn={() => {
					drawnRevision = revision;
					markPageDrawn();
				}}
			/>
		{/if}

		{#if selection.length || hits.length}
			<svg
				class="pointer-events-none absolute inset-0"
				width={innerW}
				height={innerH}
				viewBox="0 0 {size.width} {size.height}"
				aria-hidden="true"
			>
				{#each hits as { index: i, hit } (i)}
					{#each hit.quads as q, qi (qi)}
						<polygon
							points="{q[0]},{q[1]} {q[2]},{q[3]} {q[6]},{q[7]} {q[4]},{q[5]}"
							class={i === tab.search.current ? 'fill-search-current' : 'fill-search-hit'}
						/>
					{/each}
				{/each}
				{#each selection as r, ri (ri)}
					<rect x={r[0]} y={r[1]} width={r[2]! - r[0]!} height={r[3]! - r[1]!} class="fill-selection" />
				{/each}
			</svg>
		{/if}
	</div>
</div>
