<script lang="ts">
	// One page: its rendered pixels (whole, or tiles at high zoom), and overlays for the
	// text selection and search hits. Inside, everything is laid out unrotated in page
	// points scaled to CSS pixels; a CSS transform applies the view rotation.
	import { untrack } from 'svelte';
	import { SvelteSet } from 'svelte/reactivity';

	import { cancelTextDraft, commitTextDraft } from '#lib/features/annotations/actions.ts';
	import { HANDLES, handlePoint } from '#lib/features/annotations/geometry.ts';
	import { tools } from '#lib/features/annotations/state.svelte.ts';
	import { capabilities, isMarkupTool } from '#lib/features/annotations/tools.ts';
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
		/** The zoom to render pixels for; it lags `zoom` during a wheel or pinch zoom. */
		renderZoom: number;
		/** A wheel or pinch zoom is under way: request no new tiles. */
		zooming: boolean;
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
		renderZoom,
		zooming,
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
	const scale = $derived(renderScale(renderZoom, devicePixelRatio));
	const pixels = $derived(pixelSize(size, scale));
	const tiled = $derived(pixels.width * pixels.height > TILE_THRESHOLD_PIXELS);
	const underlayScale = $derived(
		Math.min(scale, Math.round(scaleForPixels(size, UNDERLAY_PIXELS) * 1000) / 1000)
	);
	const baseScale = $derived(tiled ? underlayScale : scale);
	const baseKey = $derived(`${tab.id}:${index}:${revision}:${baseScale}`);
	let drawnBase = $state('');

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

	interface Tile {
		/** Unique across scales, so a tile's canvas never shows another scale's pixels. */
		id: string;
		scale: number;
		x: number;
		y: number;
		w: number;
		h: number;
	}

	let heldTiles: Tile[] = [];
	/**
	 * Tiles intersecting the visible part of the page, in device pixels at `scale`. During a
	 * wheel or pinch zoom the set stays as it was: zooming out, the visible area grows in
	 * pixels of the held scale, and those tiles would be replaced moments later.
	 */
	const tiles: Tile[] = $derived.by(() => {
		if (zooming) return heldTiles;
		heldTiles = visibleTiles();
		return heldTiles;
	});

	function visibleTiles(): Tile[] {
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
				out.push({ id: `${scale}:${tx},${ty}`, scale, x, y, w, h });
			}
		}
		return out;
	}

	// When the scale changes, the tiles drawn at the old one stay underneath the new ones
	// until all of those have arrived, so the page does not fall back to the blurry underlay.
	const drawnTiles = new SvelteSet<string>();
	let underTiles: Tile[] = $state([]);
	let lastScale = untrack(() => scale);
	let lastTiles: Tile[] = [];
	$effect.pre(() => {
		const s = scale;
		const current = tiles;
		untrack(() => {
			if (s !== lastScale) {
				const drawn = lastTiles.filter((t) => drawnTiles.has(t.id));
				// If none of the last scale's tiles arrived, the older layer is still the best.
				if (drawn.length > 0) underTiles = drawn;
				drawnTiles.clear();
				lastScale = s;
			}
			lastTiles = current;
		});
	});
	$effect(() => {
		if (underTiles.length === 0) return;
		const covered = tiled ? tiles.every((t) => drawnTiles.has(t.id)) : drawnBase === baseKey;
		if (covered) underTiles = [];
	});
	/** Old tiles first, so the new ones are drawn over them. */
	const shownTiles = $derived([...underTiles.filter((t) => t.scale !== scale), ...(tiled ? tiles : [])]);

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
	const label = $derived(tab.displayLabels?.[index]);

	// Annotations: the selected one's outline and handles, and what is being drawn.
	const selected = $derived(tab.selectedAnnotation?.page === index ? tab.selectedAnnotationInfo : null);
	const draft = $derived(tab.draft?.page === index ? tab.draft : null);
	const handles = $derived(selected !== null && capabilities(selected, tab.flags.canAnnotate).resize);
	/** Handle size in page points: 8 screen pixels at any zoom. */
	const hs = $derived(8 / k);
	const showSelected = $derived(
		selected !== null && !(draft && (draft.kind === 'move' || draft.kind === 'text') && draft.id === selected.id)
	);
	const textDraft = $derived(draft?.kind === 'text' ? draft : null);
	const areaColor = $derived(isMarkupTool(tools.tool) ? tools.style(tools.tool).color : null);

	function onEditorInput(event: Event) {
		const d = tab.draft;
		if (d?.kind !== 'text') return;
		const el = event.currentTarget as HTMLTextAreaElement;
		tab.draft = { ...d, text: el.value };
		fitEditor(el);
	}

	function onEditorKey(event: KeyboardEvent) {
		event.stopPropagation();
		if (event.key === 'Escape') {
			event.preventDefault();
			cancelTextDraft(tab);
			tab.viewer?.focus();
		} else if (event.key === 'Enter' && event.ctrlKey) {
			event.preventDefault();
			void commitTextDraft(tab);
			tab.viewer?.focus();
		}
	}

	/** Grows the editor with its text, as the text box will. */
	function fitEditor(el: HTMLTextAreaElement) {
		el.style.height = 'auto';
		el.style.height = `${el.scrollHeight}px`;
	}

	function editor(el: HTMLTextAreaElement) {
		fitEditor(el);
		el.focus();
		el.setSelectionRange(el.value.length, el.value.length);
	}
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
		<!-- The whole page, or the low-resolution underlay beneath tiles: one canvas either
		     way, so crossing the tile threshold keeps the pixels already there. -->
		<RenderedImage
			imageKey={baseKey}
			url={pageUrl(tab.id, index, baseScale, revision)}
			{priority}
			x={0}
			y={0}
			width={innerW}
			height={innerH}
			ondrawn={(key) => {
				drawnBase = key;
				drawnRevision = revision;
				markPageDrawn();
			}}
		/>
		{#each shownTiles as t (t.id)}
			<RenderedImage
				imageKey="{tab.id}:{index}:{revision}:{t.id}"
				url={pageUrl(tab.id, index, t.scale, revision, { x: t.x, y: t.y, width: t.w, height: t.h })}
				priority={priority + 0.5}
				x={(t.x * k) / t.scale}
				y={(t.y * k) / t.scale}
				width={(t.w * k) / t.scale}
				height={(t.h * k) / t.scale}
				ondrawn={() => drawnTiles.add(t.id)}
			/>
		{/each}

		{#if selection.length || hits.length || showSelected || (draft && draft.kind !== 'text')}
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
				{#if showSelected && selected}
					{@const b = selected.bounds}
					<rect
						x={b[0]}
						y={b[1]}
						width={b[2] - b[0]}
						height={b[3] - b[1]}
						class="annotation-outline"
						vector-effect="non-scaling-stroke"
					/>
					{#if handles}
						{#each HANDLES as h (h)}
							{@const [hx, hy] = handlePoint(b, h)}
							<rect
								x={hx - hs / 2}
								y={hy - hs / 2}
								width={hs}
								height={hs}
								class="annotation-handle"
								vector-effect="non-scaling-stroke"
							/>
						{/each}
					{/if}
				{/if}
				{#if draft?.kind === 'move'}
					{@const b = draft.box}
					<rect
						x={b[0]}
						y={b[1]}
						width={b[2] - b[0]}
						height={b[3] - b[1]}
						class="annotation-outline annotation-outline-draft"
						vector-effect="non-scaling-stroke"
					/>
				{:else if draft?.kind === 'ink'}
					{@const style = tools.style('ink')}
					<polyline
						points={draft.points.join(' ')}
						fill="none"
						stroke={style.color}
						stroke-width={style.width}
						stroke-opacity={style.opacity}
						stroke-linecap="round"
						stroke-linejoin="round"
					/>
				{:else if draft?.kind === 'area'}
					{@const b = draft.box}
					<rect
						x={Math.min(b[0], b[2])}
						y={Math.min(b[1], b[3])}
						width={Math.abs(b[2] - b[0])}
						height={Math.abs(b[3] - b[1])}
						class="annotation-outline annotation-outline-draft"
						fill={areaColor ?? 'none'}
						fill-opacity={areaColor ? 0.35 : 0}
						vector-effect="non-scaling-stroke"
					/>
				{/if}
			</svg>
		{/if}

		{#if textDraft}
			{@const b = textDraft.box}
			<textarea
				data-annotation-editor
				rows="1"
				class="annotation-editor absolute"
				style:left="{b[0] * k}px"
				style:top="{b[1] * k}px"
				style:width="{(b[2] - b[0]) * k}px"
				style:min-height="{(b[3] - b[1]) * k}px"
				style:font-size="{textDraft.fontSize * k}px"
				style:color={textDraft.color}
				value={textDraft.text}
				aria-label="Text box text"
				spellcheck="true"
				oninput={onEditorInput}
				onkeydown={onEditorKey}
				onblur={() => void commitTextDraft(tab)}
				{@attach editor}
			></textarea>
		{/if}
	</div>
</div>
