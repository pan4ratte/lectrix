<script lang="ts">
	// One page: its rendered pixels (whole, or tiles at high zoom), and overlays for the
	// text selection and search hits. Inside, everything is laid out unrotated in page
	// points scaled to CSS pixels; a CSS transform turns it as the page is shown (the view
	// rotation and the page's own turn, section 6.1). The document's visual adjustments
	// (section 6.1) are a CSS filter on all of that, pixels and overlays alike, so what
	// shows on the page stays as visible as it was; the page's shadow is outside it. Its
	// colours part (greyscale, inverted) leaves the annotations out: a page with
	// annotations is then its content, with the annotations on a layer over it.
	import { untrack } from 'svelte';
	import { SvelteSet } from 'svelte/reactivity';

	import { cancelTextDraft, commitTextDraft, hiddenOnPage } from '#lib/features/annotations/actions.ts';
	import { HANDLES, handlePoint, textBoxFrame } from '#lib/features/annotations/geometry.ts';
	import { tools } from '#lib/features/annotations/state.svelte.ts';
	import { capabilities, isMarkupTool } from '#lib/features/annotations/tools.ts';
	import { pageUrl, type PageSize } from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { CSS_PX_PER_PT, turnTransform, type Rotation } from './layout.ts';
	import { blankTracker } from './perf.ts';
	import { markPageDrawn } from './perfrun.ts';
	import RenderedImage from './RenderedImage.svelte';
	import { caretBar, ordered, selectionHandles, selectionRects, type Caret } from './selection.ts';
	import { TILE_SIZE, cssPerImagePixel, isTiled, pixelSize, renderScale, snapToDevice, wholePageScale } from './zoom.ts';

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
		/** How far the page is turned on screen. */
		rotation: Rotation;
		devicePixelRatio: number;
		/** Where the box `left` and `top` are measured from begins in the window, in CSS
		 * pixels, so the page can be laid on the screen's pixels. */
		origin: { x: number; y: number };
		/** The area of the link under the pointer on this page (page points), if any. */
		linkHover: readonly [number, number, number, number] | null;
		/** The part of the page box that is on screen (plus a margin), CSS pixels relative
		 * to the page box; null when the page is off screen. */
		visibleRect: { x0: number; y0: number; x1: number; y1: number } | null;
		/** Whether any of the page is in the viewport itself (for blank tracking). */
		onscreen: boolean;
		priority: number;
		/** The text cursor, when it is on this page and shows. */
		caret: Caret | null;
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
		origin,
		linkHover,
		visibleRect,
		onscreen,
		priority,
		caret
	}: Props = $props();

	const k = $derived(zoom * CSS_PX_PER_PT);
	/** Unrotated page size in CSS pixels. */
	const innerW = $derived(size.width * k);
	const innerH = $derived(size.height * k);
	// Turned about whole device pixels, so a page's pixels stay on the screen's.
	const transform = $derived(
		turnTransform(rotation, snapToDevice(innerW, devicePixelRatio), snapToDevice(innerH, devicePixelRatio))
	);

	const revision = $derived(tab.state.revision);
	const scale = $derived(renderScale(renderZoom, devicePixelRatio));
	const pixels = $derived(pixelSize(size, scale));
	const tiled = $derived(isTiled(size, scale));
	const baseScale = $derived(wholePageScale(size, scale));
	/** The whole page's image, laid out one to one on the screen's pixels at the page's own
	 * scale (and so a fraction of a pixel past the page's edge, which it clips). */
	const basePixels = $derived(pixelSize(size, baseScale));
	const baseUnit = $derived(cssPerImagePixel(baseScale, zoom, devicePixelRatio));
	/** The annotations the annotation list filters out, which the page is drawn without. */
	const hidden = $derived(hiddenOnPage(tab, index));
	const hiddenKey = $derived(hidden.join(','));
	/** While the colours are adjusted, a page with annotations is its content, filtered,
	 * with its annotations on a layer of their own over it (section 6.1). */
	const annotLayer = $derived(tab.annotationLayerOn(index, hidden));
	/** What the page's pixels are drawn without: its annotations, or those hidden. */
	const without = $derived(annotLayer ? 'content' : hiddenKey);
	const baseKey = $derived(`${tab.id}:${index}:${revision}:${baseScale}:${without}`);
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
				// New canvases when the page turns: a tile's old pixels belong elsewhere on it.
				out.push({ id: `${scale}:${size.rotation}:${tx},${ty}`, scale, x, y, w, h });
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

	/** The annotation layer's tiles drawn, by image key. Its tiles are transparent, so only
	 * the current scale's are shown, and its whole-page image beneath until they are all
	 * drawn. */
	const drawnLayerTiles = new SvelteSet<string>();
	const layerTileKey = (t: Tile) => `${tab.id}:${index}:${revision}:${t.id}:${hiddenKey}:${annotLayer}`;
	const layerTilesDrawn = $derived(tiled && tiles.length > 0 && tiles.every((t) => drawnLayerTiles.has(layerTileKey(t))));
	$effect(() => {
		// Keys of other revisions, scales and layers are not drawn any more.
		void revision;
		void scale;
		void annotLayer;
		untrack(() => drawnLayerTiles.clear());
	});

	// Pixels on screen for the current revision (the whole page, or the tile underlay).
	let drawnRevision = $state(-1);
	const hasPixels = $derived(drawnRevision === revision);
	const trackKey = $derived(`${tab.key}:${index}`);

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

	/** The handles on the sides of the selection that are on this page, when Settings asks
	 * for them (section 6.1). */
	const sides = $derived.by(() => {
		void tab.textVersion;
		const sel = tab.selection;
		const text = tab.text(index);
		if (!app.settings?.selectionHandles || !sel || !text) return [];
		const [start, end] = ordered(sel.anchor, sel.focus);
		return selectionHandles(text, start, end);
	});

	/** Where the text cursor is drawn, and a key that starts its blink again as it moves. */
	const cursor = $derived.by(() => {
		void tab.textVersion;
		const text = tab.text(index);
		const bar = caret && text ? caretBar(text, caret) : null;
		return bar && caret ? { bar, key: `${caret.line}:${caret.offset}` } : null;
	});

	/** The area of the link under the pointer, tinted unless Settings turn that off. */
	const linkTint = $derived(app.settings?.highlightLinks === false ? null : linkHover);

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

<!-- White until its pixels come: with adjustments, that white is the filtered layer's, so an
     inverted page waits dark, and no white shows at its edges. -->
<div
	class="page absolute overflow-hidden shadow-[0_1px_3px_var(--color-page-shadow)]"
	class:bg-white={!tab.pageFilter}
	class:isolate={tab.pageTone !== null}
	style:left="{snapToDevice(left, devicePixelRatio, origin.x)}px"
	style:top="{snapToDevice(top, devicePixelRatio, origin.y)}px"
	style:width="{width}px"
	style:height="{height}px"
	data-page={index}
	role="img"
	aria-label={label ? `Page ${label} (${index + 1} of ${tab.pageCount})` : `Page ${index + 1} of ${tab.pageCount}`}
>
	<!-- Brightness and contrast act on the page as it is seen, annotations included; the
	     colours part of the filter on everything but the annotations' own layer. -->
	<div
		class="absolute top-0 left-0 origin-top-left"
		class:isolate={annotLayer}
		style:width="{innerW}px"
		style:height="{innerH}px"
		style:transform
		style:filter={tab.pageLight}
		data-page-layer
	>
		<div class="absolute inset-0" class:bg-white={tab.pageFilter} style:filter={tab.pageColour} data-page-pixels>
			<!-- The whole page, or the low-resolution underlay beneath tiles: one canvas either
			     way, so crossing the tile threshold keeps the pixels already there. -->
			<RenderedImage
				imageKey={baseKey}
				url={pageUrl(tab.id, index, baseScale, revision, undefined, undefined, hidden, annotLayer ? 'content' : undefined)}
				{priority}
				x={0}
				y={0}
				width={basePixels.width * baseUnit}
				height={basePixels.height * baseUnit}
				rotation={size.rotation}
				ondrawn={(key) => {
					drawnBase = key;
					drawnRevision = revision;
					markPageDrawn();
				}}
			/>
			{#each shownTiles as t (t.id)}
				{@const unit = cssPerImagePixel(t.scale, zoom, devicePixelRatio)}
				<RenderedImage
					imageKey="{tab.id}:{index}:{revision}:{t.id}:{without}"
					url={pageUrl(
						tab.id,
						index,
						t.scale,
						revision,
						{ x: t.x, y: t.y, width: t.w, height: t.h },
						undefined,
						hidden,
						annotLayer ? 'content' : undefined
					)}
					priority={priority + 0.5}
					x={t.x * unit}
					y={t.y * unit}
					width={t.w * unit}
					height={t.h * unit}
					ondrawn={() => drawnTiles.add(t.id)}
				/>
			{/each}
		</div>
		{#if annotLayer}
			<!-- The annotations, unfiltered: as they are under greyscale, and made light for an
			     inverted page, which they are laid over as light. -->
			<div class="absolute inset-0" class:mix-blend-screen={annotLayer === 'annots-dark'} data-annotation-layer={annotLayer}>
				<!-- Beneath tiles only until they are drawn: being transparent, they would show
				     its blurrier annotations around theirs. -->
				<RenderedImage
					imageKey="{tab.id}:{index}:{revision}:{baseScale}:{hiddenKey}:{annotLayer}"
					url={pageUrl(tab.id, index, baseScale, revision, undefined, undefined, hidden, annotLayer)}
					priority={priority + 0.25}
					x={0}
					y={0}
					width={basePixels.width * baseUnit}
					height={basePixels.height * baseUnit}
					rotation={size.rotation}
					transparent
					hidden={layerTilesDrawn}
				/>
				{#each tiled ? tiles : [] as t (t.id)}
					{@const unit = cssPerImagePixel(t.scale, zoom, devicePixelRatio)}
					<RenderedImage
						imageKey={layerTileKey(t)}
						url={pageUrl(
							tab.id,
							index,
							t.scale,
							revision,
							{ x: t.x, y: t.y, width: t.w, height: t.h },
							undefined,
							hidden,
							annotLayer
						)}
						priority={priority + 0.75}
						x={t.x * unit}
						y={t.y * unit}
						width={t.w * unit}
						height={t.h * unit}
						transparent
						ondrawn={(key) => drawnLayerTiles.add(key)}
					/>
				{/each}
			</div>
		{/if}

		{#if selection.length || hits.length || showSelected || linkTint || cursor || (draft && draft.kind !== 'text')}
			<svg
				class="pointer-events-none absolute inset-0"
				style:filter={tab.pageColour}
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
				{#if linkTint}
					<rect
						x={linkTint[0]}
						y={linkTint[1]}
						width={linkTint[2] - linkTint[0]}
						height={linkTint[3] - linkTint[1]}
						rx={2 / k}
						class="fill-link-hover"
						data-link-hover
					/>
				{/if}
				{#each selection as r, ri (ri)}
					<rect x={r[0]} y={r[1]} width={r[2]! - r[0]!} height={r[3]! - r[1]!} class="fill-selection" />
				{/each}
				{#each sides as s (s.side)}
					<!-- A bar down the side, with a knob at the top of the start and the bottom of the
					     end: the same size on screen at any zoom. -->
					<line
						x1={s.box[0]}
						y1={s.box[1]}
						x2={s.box[2]}
						y2={s.box[3]}
						class="selection-handle"
						data-selection-handle={s.side}
						vector-effect="non-scaling-stroke"
					/>
					<circle cx={s.knob[0]} cy={s.knob[1]} r={4.5 / k} class="selection-handle-knob" />
				{/each}
				{#if cursor}
					{#key cursor.key}
						<line
							x1={cursor.bar[0]}
							y1={cursor.bar[1]}
							x2={cursor.bar[2]}
							y2={cursor.bar[3]}
							class="text-caret"
							data-text-caret
							vector-effect="non-scaling-stroke"
						/>
					{/key}
				{/if}
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
			<!-- Turned as the box's text is: for a new box, upright as the page is shown. Its
			     first line starts at the editor's corner, and it grows down from there. -->
			{@const frame = textBoxFrame(textDraft.box, textDraft.turn)}
			<textarea
				data-annotation-editor
				rows="1"
				class="annotation-editor absolute origin-top-left"
				style:left="{frame.x * k}px"
				style:top="{frame.y * k}px"
				style:width="{frame.across * k}px"
				style:min-height="{frame.down * k}px"
				style:transform={textDraft.turn ? `rotate(${-textDraft.turn}deg)` : undefined}
				style:font-size="{textDraft.fontSize * k}px"
				style:color={textDraft.color}
				style:filter={tab.pageColour}
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
	<!-- Sepia (section 6.1): the paper's tone, multiplied into the page as it is seen. -->
	{#if tab.pageTone !== null}
		<div class="page-tone" style:opacity={tab.pageTone} data-page-tone aria-hidden="true"></div>
	{/if}
</div>
