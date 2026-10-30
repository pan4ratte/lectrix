<script lang="ts">
	// A presentation's page (section 6.1), as large as fits the box it is given, in its
	// middle: in the window presenting, on an audience screen, and small in the presenter
	// view. A page shows once its pixels are drawn, the page before staying until then, so
	// none ever comes blank; it comes with its transition (transitions.ts), if it has one
	// and `animate` allows. The pages kept beside it are rendered out of sight. The visual
	// adjustments act as in the viewer (PageView.svelte).
	import { untrack } from 'svelte';
	import { SvelteSet } from 'svelte/reactivity';

	import { motionMs } from '#lib/components/panes.ts';
	import { CSS_PX_PER_PT, turnTransform } from '#lib/features/viewer/layout.ts';
	import RenderedImage from '#lib/features/viewer/RenderedImage.svelte';
	import { pixelSize, renderScale, snapToDevice } from '#lib/features/viewer/zoom.ts';
	import { pageUrl } from '#lib/ipc/index.ts';

	import { placePage } from './input.ts';
	import type { StagePage, StageState } from './stage.ts';
	import { transitionFrame, type Frame, type PageLook } from './transitions.ts';

	/** How long a page whose pixels don't come is waited for before it shows anyway. */
	const WAIT_MS = 800;

	interface Props {
		stage: StageState;
		/** The box the page fits, CSS pixels. */
		width: number;
		height: number;
		/** Transitions play (not in the presenter view's small pages). */
		animate?: boolean;
		/** Render priority of the page shown; those kept beside it come after. */
		priority?: number;
		/** A name for the page for assistive technology, else it is left out. */
		label?: (index: number) => string;
	}

	let { stage, width, height, animate = true, priority = -3, label }: Props = $props();

	let dpr = $state(typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1);
	$effect(() => {
		const update = () => (dpr = window.devicePixelRatio || 1);
		window.addEventListener('resize', update);
		return () => window.removeEventListener('resize', update);
	});

	function place(page: StagePage) {
		const placed = placePage(page, page.turn, width, height);
		const scale = renderScale(placed.zoom, dpr);
		const pixels = pixelSize(page, scale);
		const k = placed.zoom * CSS_PX_PER_PT;
		return {
			placed,
			scale,
			k,
			// The image one to one on the screen's pixels, and the page on whole ones.
			imageW: pixels.width / dpr,
			imageH: pixels.height / dpr,
			left: snapToDevice(placed.left, dpr),
			top: snapToDevice(placed.top, dpr),
			transform: turnTransform(page.turn, snapToDevice(page.width * k, dpr), snapToDevice(page.height * k, dpr))
		};
	}

	const baseKey = (page: StagePage, scale: number) =>
		`present:${stage.docId}:${page.index}:${stage.revision}:${scale}:${page.layer ? 'content' : page.hidden.join(',')}`;

	/** The pages whose pixels are drawn, by the key of their image. */
	const drawn = new SvelteSet<string>();

	/** The page on screen, and the one going while a transition plays. */
	let displayed = $state<number | null>(null);
	let going = $state<number | null>(null);
	let frame = $state<Frame | null>(null);
	let raf = 0;
	let waiting: ReturnType<typeof setTimeout> | undefined;

	function isDrawn(index: number): boolean {
		const page = stage.pages.find((p) => p.index === index);
		return page !== undefined && drawn.has(baseKey(page, place(page).scale));
	}

	function stopTransition() {
		cancelAnimationFrame(raf);
		raf = 0;
		frame = null;
		going = null;
	}

	function show(index: number) {
		clearTimeout(waiting);
		waiting = undefined;
		if (index === displayed) return;
		stopTransition();
		const transition = stage.change.transition;
		const ms = transition ? motionMs(transition.seconds * 1000) : 0;
		if (displayed === null || !animate || !stage.shown || !transition || ms <= 0 || stage.blank) {
			displayed = index;
			return;
		}
		going = displayed;
		displayed = index;
		const start = performance.now();
		const step = (now: number) => {
			const t = (now - start) / ms;
			if (t >= 1) {
				stopTransition();
				return;
			}
			frame = transitionFrame(transition, t, width, height);
			raf = requestAnimationFrame(step);
		};
		frame = transitionFrame(transition, 0, width, height);
		raf = requestAnimationFrame(step);
	}

	// The page asked for shows once it is drawn, or after a moment without its pixels.
	$effect(() => {
		void stage.change.seq;
		const target = stage.current;
		const ready = isDrawn(target);
		untrack(() => {
			if (target === displayed) {
				clearTimeout(waiting);
				waiting = undefined;
			} else if (ready) show(target);
			else if (waiting === undefined) waiting = setTimeout(() => show(stage.current), WAIT_MS);
		});
	});

	// Forget the drawn pages no longer kept.
	$effect(() => {
		const keep = new Set(stage.pages.map((p) => baseKey(p, place(p).scale)));
		untrack(() => {
			for (const key of [...drawn]) if (!keep.has(key)) drawn.delete(key);
		});
	});

	$effect(() => () => {
		cancelAnimationFrame(raf);
		clearTimeout(waiting);
	});

	/** A look as a style attribute; masks with the prefix WebKitGTK still needs. */
	function styleOf(look: PageLook | undefined): string {
		if (!look) return '';
		const parts: string[] = [];
		if (look.opacity !== undefined) parts.push(`opacity: ${look.opacity}`);
		if (look.transform) parts.push(`transform: ${look.transform}`);
		if (look.clipPath) parts.push(`clip-path: ${look.clipPath}`);
		if (look.mask) parts.push(`mask-image: ${look.mask}`, `-webkit-mask-image: ${look.mask}`);
		return parts.join('; ');
	}

	const priorityOf = (index: number) =>
		index === stage.current ? priority : index === stage.current + 1 ? priority + 1 : priority + 2;
</script>

{#each stage.pages as page (page.index)}
	{@const p = place(page)}
	{@const onScreen = stage.shown && (page.index === displayed || page.index === going)}
	{@const isGoing = page.index === going && frame !== null}
	<div
		class="absolute overflow-hidden will-change-transform"
		class:invisible={!onScreen}
		class:bg-white={!stage.look.adjusted}
		class:isolate={stage.look.tone !== null}
		style="left: {p.left}px; top: {p.top}px; width: {p.placed.width}px; height: {p.placed.height}px; z-index: {isGoing
			? frame?.outgoingOnTop
				? 3
				: 1
			: 2}; {page.index === displayed && frame ? styleOf(frame.incoming) : isGoing ? styleOf(frame?.outgoing) : ''}"
		data-presentation-page={page.index}
		data-shown={onScreen && page.index === displayed ? '' : undefined}
		role={label && page.index === displayed ? 'img' : undefined}
		aria-label={label && page.index === displayed ? label(page.index) : undefined}
		aria-hidden={label && page.index === displayed ? undefined : 'true'}
	>
		<div
			class="absolute top-0 left-0 origin-top-left"
			class:isolate={page.layer !== null}
			style:width="{page.width * p.k}px"
			style:height="{page.height * p.k}px"
			style:transform={p.transform}
			style:filter={stage.look.light}
		>
			<div class="absolute inset-0" class:bg-white={stage.look.adjusted} style:filter={stage.look.colour}>
				<RenderedImage
					imageKey={baseKey(page, p.scale)}
					url={pageUrl(stage.docId, page.index, p.scale, stage.revision, undefined, stage.imageFormat, page.hidden, page.layer ? 'content' : undefined)}
					priority={priorityOf(page.index)}
					x={0}
					y={0}
					width={p.imageW}
					height={p.imageH}
					rotation={page.rotation}
					ondrawn={(key) => drawn.add(key)}
				/>
			</div>
			{#if page.layer}
				<div class="absolute inset-0" class:mix-blend-screen={page.layer === 'annots-dark'}>
					<RenderedImage
						imageKey="{baseKey(page, p.scale)}:{page.layer}"
						url={pageUrl(stage.docId, page.index, p.scale, stage.revision, undefined, stage.imageFormat, page.hidden, page.layer)}
						priority={priorityOf(page.index) + 0.5}
						x={0}
						y={0}
						width={p.imageW}
						height={p.imageH}
						rotation={page.rotation}
						transparent
					/>
				</div>
			{/if}
		</div>
		{#if stage.look.tone !== null}
			<div class="page-tone" style:opacity={stage.look.tone} aria-hidden="true"></div>
		{/if}
	</div>
{/each}

{#if stage.blank}
	<div
		class={['absolute inset-0 z-10', stage.blank === 'black' ? 'bg-presentation' : 'bg-presentation-white']}
		data-presentation-blank={stage.blank}
	></div>
{/if}
