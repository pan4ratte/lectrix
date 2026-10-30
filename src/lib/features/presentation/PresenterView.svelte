<script lang="ts">
	// The presenter view (section 6.1), in the window presenting while the pages show on
	// another screen: the page shown, large; the next page; the page's notes (the comments
	// of its annotations); how long the presentation has run, and the time; and the controls.
	import { ArrowLeftRight, ChevronLeft, ChevronRight, Pause, Play, RotateCcw, Square, X } from '@lucide/svelte';
	import { onMount } from 'svelte';

	import { pagePosition } from '#lib/features/viewer/pagebox.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { StageGestures } from './gestures.ts';
	import PageStage from './PageStage.svelte';
	import { presentation } from './presentation.svelte.ts';
	import { linkIndexAt, type StageState } from './stage.ts';

	interface Props {
		tab: DocTab;
		/** What the audience screen shows. */
		stage: StageState;
		/** Follows a link of the page shown, by its index. */
		onFollow: (link: number) => void;
	}

	let { tab, stage, onFollow }: Props = $props();

	let now = $state(Date.now());
	onMount(() => {
		const timer = setInterval(() => (now = Date.now()), 250);
		return () => clearInterval(timer);
	});

	let currentBox: HTMLDivElement | undefined = $state();
	let currentW = $state(0);
	let currentH = $state(0);
	let nextW = $state(0);
	let nextH = $state(0);

	/** The page shown, as the audience sees it but never blank: this screen says so instead. */
	const currentStage = $derived<StageState>({ ...stage, blank: null });
	const next = $derived(stage.current + 1 < tab.pageCount ? stage.current + 1 : null);
	const nextStage = $derived<StageState | null>(
		next === null
			? null
			: {
					...stage,
					current: next,
					pages: stage.pages.filter((p) => p.index === next),
					blank: null,
					change: { seq: 0, from: null, transition: null }
				}
	);

	/** The page's notes: the comments of its annotations (replies under what they answer
	 * come after it), top to bottom. */
	const notes = $derived(
		tab
			.annotationsOn(stage.current)
			.filter((a) => a.contents.trim() !== '')
			.toSorted((a, b) => a.bounds[1] - b.bounds[1] || a.bounds[0] - b.bounds[0])
	);

	function clock(ms: number): string {
		const total = Math.floor(ms / 1000);
		const h = Math.floor(total / 3600);
		const m = Math.floor((total % 3600) / 60);
		const s = total % 60;
		const two = (v: number) => String(v).padStart(2, '0');
		return h > 0 ? `${h}:${two(m)}:${two(s)}` : `${two(m)}:${two(s)}`;
	}

	const timeOfDay = $derived(new Date(now).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }));
	const position = $derived(pagePosition(stage.current, tab.pageCount, tab.displayLabels));

	const gestures = new StageGestures({
		active: () => presentation.shown,
		linkAt: (x, y) => {
			const page = stage.pages.find((p) => p.index === stage.current);
			const box = currentBox?.getBoundingClientRect();
			return page && box ? linkIndexAt(page, currentW, currentH, x - box.left, y - box.top) : null;
		},
		step: (direction) => presentation.step(direction),
		follow: (link) => onFollow(link),
		onControl: () => false
	});
</script>

<div class="flex h-full flex-col gap-3 p-4 text-fg" data-presenter>
	<!-- The time: how long the presentation has run, which can be paused and started over,
	     and the time of day. -->
	<div class="flex shrink-0 items-center gap-3">
		<span class="text-3xl font-semibold tabular-nums" aria-label="Time presenting" data-presenter-timer>
			{clock(presentation.elapsed(now))}
		</span>
		<button
			type="button"
			class="icon-button"
			aria-label={presentation.timerPaused === null ? 'Pause the timer' : 'Start the timer again'}
			data-tip={presentation.timerPaused === null ? 'Pause the timer' : 'Start the timer again'}
			onclick={() => presentation.toggleTimer()}
		>
			{#if presentation.timerPaused === null}
				<Pause size={16} aria-hidden="true" />
			{:else}
				<Play size={16} aria-hidden="true" />
			{/if}
		</button>
		<button
			type="button"
			class="icon-button"
			aria-label="Reset the timer"
			data-tip="Reset the timer"
			onclick={() => presentation.resetTimer()}
		>
			<RotateCcw size={16} aria-hidden="true" />
		</button>
		<span class="flex-1"></span>
		<span class="text-lg text-fg-muted tabular-nums" aria-label="Time of day">{timeOfDay}</span>
	</div>

	<div class="flex min-h-0 flex-1 gap-4">
		<!-- The page shown. A click on it turns on, or follows a link, as on the screen. -->
		<section class="flex min-w-0 flex-[3] flex-col gap-2" aria-label="Current page">
			<!-- svelte-ignore a11y_click_events_have_key_events -->
			<!-- svelte-ignore a11y_no_static_element_interactions -->
			<div
				bind:this={currentBox}
				bind:clientWidth={currentW}
				bind:clientHeight={currentH}
				class="relative min-h-0 flex-1 overflow-hidden rounded-panel bg-presentation"
				data-presenter-current
				onclick={gestures.click}
				oncontextmenu={gestures.contextMenu}
				onwheel={gestures.wheel}
			>
				{#if currentW > 0}
					<PageStage stage={currentStage} width={currentW} height={currentH} animate={false} />
				{/if}
				{#if stage.blank}
					<div class="absolute inset-0 z-20 flex items-center justify-center bg-[var(--lectrix-overlay)]">
						<span class="rounded-panel bg-surface-raised px-3 py-2 text-base" data-presenter-blank>
							{stage.blank === 'black' ? 'The screen is black' : 'The screen is white'}
						</span>
					</div>
				{/if}
			</div>
			<p class="text-center text-base tabular-nums" aria-live="polite" data-presentation-position>
				{presentation.typed ? `Page ${presentation.typed}` : position}
			</p>
		</section>

		<div class="flex min-w-0 flex-[2] flex-col gap-4">
			<section class="flex min-h-0 flex-1 flex-col gap-2" aria-label="Next page">
				<h2 class="text-sm font-semibold text-fg-muted">Next</h2>
				<div
					bind:clientWidth={nextW}
					bind:clientHeight={nextH}
					class="relative min-h-0 flex-1 overflow-hidden rounded-panel bg-presentation"
					data-presenter-next
				>
					{#if nextStage && nextW > 0}
						<PageStage stage={nextStage} width={nextW} height={nextH} animate={false} priority={-2} />
					{:else if !nextStage}
						<p class="absolute inset-0 flex items-center justify-center text-base text-presentation-white">
							End of the document
						</p>
					{/if}
				</div>
			</section>
			<section class="flex min-h-0 flex-1 flex-col gap-2" aria-label="Notes">
				<h2 class="text-sm font-semibold text-fg-muted">Notes</h2>
				<div class="min-h-0 flex-1 overflow-x-hidden overflow-y-auto rounded-panel border border-line bg-surface p-3" data-presenter-notes>
					{#each notes as note (note.id)}
						<p class="mb-3 text-base whitespace-pre-wrap select-text last:mb-0">{note.contents}</p>
					{:else}
						<p class="text-fg-muted">
							No notes on this page. The comments of a page's annotations, such as its sticky notes, show here.
						</p>
					{/each}
				</div>
			</section>
		</div>
	</div>

	<div class="flex shrink-0 items-center justify-center gap-1" role="toolbar" aria-label="Presentation" data-presentation-controls>
		<button
			type="button"
			class="icon-button"
			aria-label="Previous page"
			data-tip="Previous page"
			disabled={stage.current <= 0}
			onclick={() => presentation.step(-1)}
		>
			<ChevronLeft size={16} aria-hidden="true" />
		</button>
		<button
			type="button"
			class="icon-button"
			aria-label="Next page"
			data-tip="Next page"
			disabled={stage.current >= tab.pageCount - 1}
			onclick={() => presentation.step(1)}
		>
			<ChevronRight size={16} aria-hidden="true" />
		</button>
		<span class="mx-1 h-6 w-px shrink-0 bg-line" aria-hidden="true"></span>
		<button
			type="button"
			class="icon-button"
			aria-label="Black screen"
			aria-pressed={stage.blank === 'black'}
			data-tip="Black screen (B)"
			onclick={() => presentation.setBlank(stage.blank === 'black' ? null : 'black')}
		>
			<Square size={16} aria-hidden="true" fill="currentColor" />
		</button>
		<button
			type="button"
			class="icon-button"
			aria-label="White screen"
			aria-pressed={stage.blank === 'white'}
			data-tip="White screen (W)"
			onclick={() => presentation.setBlank(stage.blank === 'white' ? null : 'white')}
		>
			<Square size={16} aria-hidden="true" />
		</button>
		<button
			type="button"
			class="icon-button"
			aria-label="Swap screens"
			data-tip="Swap screens"
			onclick={() => void presentation.swapScreens()}
		>
			<ArrowLeftRight size={16} aria-hidden="true" />
		</button>
		<span class="mx-1 h-6 w-px shrink-0 bg-line" aria-hidden="true"></span>
		<button
			type="button"
			class="icon-button"
			aria-label="End presentation"
			data-tip={app.hint('End presentation', 'presentation')}
			onclick={() => void presentation.stop()}
		>
			<X size={16} aria-hidden="true" />
		</button>
	</div>
</div>
