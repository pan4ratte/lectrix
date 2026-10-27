<script lang="ts">
	// The Compare view (section 6.13), as ABBYY FineReader's Compare Documents lays it out:
	// the two documents side by side, each in a pane of its own (empty until a file is
	// opened or dropped there, or a document open in a tab is picked), read only, with the
	// differences marked on their pages; a bar to step through the differences and zoom;
	// and at the right the panel with the options and the list of differences, which
	// resizes from its edge and keeps its width. The panes scroll and zoom together.
	import { ArrowLeftRight, ChevronDown, ChevronUp, FileText, FolderOpen, Link2, Link2Off, Save, X, ZoomIn, ZoomOut } from '@lucide/svelte';
	import { untrack } from 'svelte';

	import PaneResizer from '#lib/components/PaneResizer.svelte';
	import { COMPARE_LIMITS, shownWidth } from '#lib/components/panes.ts';
	import Viewer from '#lib/features/viewer/Viewer.svelte';
	import { app } from '#lib/stores/app.svelte.ts';

	import type { Side } from './compare.ts';
	import type { CompareState } from './compare.svelte.ts';
	import ComparePanel from './ComparePanel.svelte';

	let { compare }: { compare: CompareState } = $props();

	const SLOTS = [
		{ slot: 0, side: 'old', title: 'First document', hint: 'The earlier version, as a rule.' },
		{ slot: 1, side: 'new', title: 'Second document', hint: 'The later version, compared with the first.' }
	] as const;

	let windowWidth = $state(typeof window === 'undefined' ? 0 : window.innerWidth);
	const panelWidth = $derived(shownWidth(app.compareWidth, COMPARE_LIMITS, windowWidth));

	/** The pane the zoom buttons act on: the one pressed last. */
	let focus = $state<Side>('old');
	const focused = $derived(compare.view(focus) ?? compare.view('old') ?? compare.view('new'));

	/** The documents open in tabs, to pick one for an empty slot. */
	const openDocs = $derived(app.documents.filter((d) => !d.untitled));

	// Either pane's zoom changed: the other takes it.
	$effect(() => {
		for (const view of compare.views) if (view) void [view.zoom, view.zoomMode];
		untrack(() => compare.zoomed());
	});
</script>

<svelte:window bind:innerWidth={windowWidth} />

<div class="flex min-h-0 flex-1">
	<section class="flex min-w-0 flex-1 flex-col" aria-label="Compare documents">
		<div class="flex h-[40px] shrink-0 items-center gap-1 border-b border-line bg-chrome px-2" role="toolbar" aria-label="Compare">
			<button
				type="button"
				class="icon-button"
				aria-label="Previous difference"
				data-tip={app.hint('Previous difference', 'previousDifference')}
				disabled={compare.listed.length === 0}
				onclick={() => compare.step(-1)}
			>
				<ChevronUp size={16} aria-hidden="true" />
			</button>
			<button
				type="button"
				class="icon-button"
				aria-label="Next difference"
				data-tip={app.hint('Next difference', 'nextDifference')}
				disabled={compare.listed.length === 0}
				onclick={() => compare.step(1)}
			>
				<ChevronDown size={16} aria-hidden="true" />
			</button>
			<span class="min-w-[88px] px-1 text-sm text-fg-muted" aria-live="polite">
				{#if compare.position.count > 0 && compare.position.at >= 0}
					{(compare.position.at + 1).toLocaleString()} of {compare.position.count.toLocaleString()}
				{:else if compare.result}
					{compare.position.count.toLocaleString()} found
				{/if}
			</span>
			<span class="mx-1 h-5 w-px bg-line" aria-hidden="true"></span>
			<button
				type="button"
				class="icon-button"
				aria-label="Zoom out"
				data-tip={app.hint('Zoom out', 'zoomOut')}
				disabled={!focused}
				onclick={() => focused?.viewer?.zoomStep(-1)}
			>
				<ZoomOut size={16} aria-hidden="true" />
			</button>
			<button
				type="button"
				class="icon-button"
				aria-label="Zoom in"
				data-tip={app.hint('Zoom in', 'zoomIn')}
				disabled={!focused}
				onclick={() => focused?.viewer?.zoomStep(1)}
			>
				<ZoomIn size={16} aria-hidden="true" />
			</button>
			<button type="button" class="bar-button" disabled={!focused} onclick={() => focused?.viewer?.fit('fitWidth')}>
				Fit width
			</button>
			<span class="mx-1 h-5 w-px bg-line" aria-hidden="true"></span>
			<button
				type="button"
				class="icon-button tool-button"
				aria-pressed={compare.locked}
				aria-label="Scroll together"
				data-tip={compare.locked ? 'Scroll the documents separately' : 'Scroll the documents together'}
				onclick={() => compare.setLocked(!compare.locked)}
			>
				{#if compare.locked}<Link2 size={16} aria-hidden="true" />{:else}<Link2Off size={16} aria-hidden="true" />{/if}
			</button>
			<button
				type="button"
				class="icon-button"
				aria-label="Swap the documents"
				data-tip="Swap the documents"
				disabled={compare.running || (!compare.views[0] && !compare.views[1])}
				onclick={() => compare.swap()}
			>
				<ArrowLeftRight size={16} aria-hidden="true" />
			</button>
		</div>

		<div class="flex min-h-0 flex-1" data-compare-panes>
			{#each SLOTS as s (s.slot)}
				{@const view = compare.views[s.slot]}
				{#if s.slot === 1}
					<div class="w-px shrink-0 bg-line" aria-hidden="true"></div>
				{/if}
				<section
					class="flex min-h-0 min-w-0 flex-1 basis-0 flex-col"
					aria-label={s.title}
					data-compare-pane={s.side}
					onpointerdowncapture={() => (focus = s.side)}
					onfocusin={() => (focus = s.side)}
				>
					<div class="compare-header" class:compare-header-focused={view !== null && focus === s.side}>
						<span class="shrink-0 text-fg-muted">{s.title}</span>
						{#if view}
							<span class="min-w-0 flex-1 truncate font-semibold" data-tip={view.path}>{view.name}</span>
							<button
								type="button"
								class="icon-button shrink-0"
								aria-label="Save {view.name} with the differences"
								data-tip="Save with the differences marked…"
								disabled={!compare.result}
								onclick={() => void compare.saveAnnotated(s.side)}
							>
								<Save size={14} aria-hidden="true" />
							</button>
							<button
								type="button"
								class="icon-button shrink-0"
								aria-label="Open another document as the {s.title.toLowerCase()}"
								data-tip="Open another document…"
								disabled={compare.running || compare.opening[s.slot]}
								onclick={() => void compare.open(s.slot)}
							>
								<FolderOpen size={14} aria-hidden="true" />
							</button>
							<button
								type="button"
								class="icon-button shrink-0"
								aria-label="Remove {view.name}"
								data-tip="Remove"
								disabled={compare.running}
								onclick={() => compare.remove(s.slot)}
							>
								<X size={14} aria-hidden="true" />
							</button>
						{:else}
							<span class="flex-1"></span>
						{/if}
					</div>
					{#if view}
						{#key view.key}
							<div class="relative flex min-h-0 min-w-0 flex-1 flex-col">
								<Viewer tab={view} />
							</div>
						{/key}
					{:else}
						<div class="compare-slot" data-compare-slot={s.slot}>
							<FileText size={40} aria-hidden="true" class="text-fg-muted" />
							<h2 class="text-lg font-semibold">{s.title}</h2>
							<p class="max-w-xs text-fg-muted">{s.hint} Drop a PDF here, or open one.</p>
							<button
								type="button"
								class="button button-primary gap-2"
								disabled={compare.opening[s.slot]}
								onclick={() => void compare.open(s.slot)}
							>
								<FolderOpen size={16} aria-hidden="true" />
								{compare.opening[s.slot] ? 'Opening…' : 'Open…'}
							</button>
							{#if openDocs.length > 0}
								<p class="mt-2 text-sm text-fg-muted">Or a document that is open:</p>
								<div class="compare-open-docs">
									{#each openDocs.slice(0, 6) as doc (doc.id)}
										<button
											type="button"
											class="compare-open-doc"
											disabled={compare.opening[s.slot]}
											data-tip={doc.path}
											onclick={() => void compare.openTab(s.slot, doc.id)}
										>
											<FileText size={14} aria-hidden="true" class="shrink-0 text-fg-muted" />
											<span class="truncate">{doc.name}</span>
										</button>
									{/each}
								</div>
							{/if}
						</div>
					{/if}
				</section>
			{/each}
		</div>
	</section>

	<aside
		class="relative flex shrink-0 flex-col border-l border-line bg-chrome"
		style:width="{panelWidth}px"
		aria-label="Compare options and differences"
		data-compare-panel
	>
		<ComparePanel {compare} />
		<PaneResizer side="right" bind:width={app.compareWidth} shown={panelWidth} limits={COMPARE_LIMITS} label="Compare options and differences" />
	</aside>
</div>
