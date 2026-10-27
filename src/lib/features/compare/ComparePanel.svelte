<script lang="ts">
	// The Compare view's panel (section 6.13), as FineReader's right-hand pane: a Compare tab
	// with the options and the Compare button (and the progress while it runs), and a
	// Differences tab listing what was found, in FineReader's groups, to step through,
	// ignore, copy, and save as a report.
	import { ChevronDown, ChevronUp, FileDiff, FileText, ListFilter, ListTree, Minus, Pencil, Plus, SlidersHorizontal, Undo2, X } from '@lucide/svelte';
	import { ContextMenu } from 'bits-ui';
	import { tick } from 'svelte';
	import { slide } from 'svelte/transition';

	import Dropdown from '#lib/components/Dropdown.svelte';
	import { motionMs } from '#lib/components/panes.ts';
	import ProgressBar from '#lib/components/ProgressBar.svelte';
	import { toolMark } from '#lib/features/annotations/toolMark.ts';
	import type { CompareMode, Difference, DifferenceKind } from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	import { KIND_NAMES, countText, pageName, progressText, sideText } from './compare.ts';
	import type { CompareState } from './compare.svelte.ts';

	let { compare }: { compare: CompareState } = $props();

	const MODES: { value: CompareMode; label: string }[] = [
		{ value: 'automatic', label: 'Text and images (automatic)' },
		{ value: 'text', label: 'Text only' },
		{ value: 'visual', label: 'Images only' }
	];

	const result = $derived(compare.result);
	const progress = $derived(progressText(compare.progress));
	const counts = $derived.by(() => {
		const r = compare.result;
		const out = { inserted: 0, deleted: 0, changed: 0 };
		if (!r) return out;
		r.differences.forEach((d, i) => {
			if (!compare.ignored.has(i)) out[d.kind]++;
		});
		return out;
	});
	const shownCount = $derived(result ? result.differences.length - compare.ignored.size : 0);

	const TABS = [
		{ id: 'compare', label: 'Compare', panel: 'compare-options', icon: SlidersHorizontal },
		{ id: 'differences', label: 'Differences', panel: 'compare-differences', icon: FileDiff }
	] as const;

	/** The arrow keys, Home and End move between the tabs and show the one they reach. */
	function onTabKey(event: KeyboardEvent, index: number) {
		const last = TABS.length - 1;
		const to: Record<string, number> = {
			ArrowRight: index === last ? 0 : index + 1,
			ArrowLeft: index === 0 ? last : index - 1,
			Home: 0,
			End: last
		};
		const next = to[event.key];
		if (next === undefined) return;
		event.preventDefault();
		const tab = TABS[next]!;
		if (tab.id === 'differences' && !compared) return;
		compare.panel = tab.id;
		document.querySelector<HTMLElement>(`[data-compare-panel-tab="${tab.id}"]`)?.focus();
	}

	/** The Differences tab opens once there are differences to list: not before the documents
	 * are compared, nor while they are compared again. */
	const compared = $derived(compare.result !== null);

	function setOption(key: 'punctuation' | 'oneLetter' | 'margins', on: boolean) {
		compare.settings = { ...compare.settings, [key]: on };
	}

	// ----- the list -----

	let list: HTMLDivElement | undefined = $state();
	/** The row the context menu was opened on. */
	let contextId = $state<number | null>(null);

	const rowId = (id: number) => `compare-difference-${id}`;

	/** The rows a command acts on: the selected ones, if `id` is among them; else `id`. */
	function targets(id: number): number[] {
		return compare.picked.has(id) ? [...compare.picked] : [id];
	}

	function onRowDown(event: PointerEvent, id: number) {
		if (event.button === 2) {
			contextId = id;
			if (!compare.picked.has(id)) compare.select(id, { reveal: 'none' });
			return;
		}
		if (event.button !== 0) return;
		compare.select(id, { add: event.ctrlKey || event.metaKey, range: event.shiftKey });
	}

	async function showCurrent() {
		await tick();
		if (compare.current !== null) document.getElementById(rowId(compare.current))?.scrollIntoView({ block: 'nearest' });
	}

	// The row selected stays in view as the selection moves (keys, the view bar, a mark
	// clicked on a page).
	$effect(() => {
		void compare.current;
		void showCurrent();
	});

	function onListKey(event: KeyboardEvent) {
		if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
			event.preventDefault();
			compare.step(event.key === 'ArrowDown' ? 1 : -1);
		} else if (event.key === 'Home' || event.key === 'End') {
			event.preventDefault();
			const id = event.key === 'Home' ? compare.listed[0] : compare.listed[compare.listed.length - 1];
			if (id !== undefined) compare.select(id);
		} else if (event.key === 'Delete' && compare.current !== null) {
			event.preventDefault();
			compare.ignore(targets(compare.current));
		} else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'c' && compare.current !== null) {
			event.preventDefault();
			void compare.copy(targets(compare.current));
		} else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'a') {
			event.preventDefault();
			compare.picked = new Set(compare.listed);
		}
	}

	function pages(d: Difference): string {
		const [a, b] = compare.views;
		const old = pageName(d.old.page, a?.displayLabels ?? null);
		const nw = pageName(d.new.page, b?.displayLabels ?? null);
		return `Page ${old} → ${nw}`;
	}

	// ----- the filter -----

	/** The filter's kinds, in the list's order of icons. */
	const KINDS: readonly DifferenceKind[] = ['inserted', 'deleted', 'changed'];
	let filterOpen = $state(false);
	/** The filter opens and shuts as the annotation list's does (in under 150 ms). */
	const reveal = () => ({ duration: motionMs(140) });

	function toggleKind(kind: DifferenceKind): Set<DifferenceKind> {
		const next = new Set(compare.kinds);
		if (next.has(kind)) next.delete(kind);
		else next.add(kind);
		return next;
	}

	function kindIcon(kind: Difference['kind']) {
		return kind === 'inserted' ? Plus : kind === 'deleted' ? Minus : Pencil;
	}
</script>

<div class="flex h-full min-h-0 flex-col">
	<!-- The tabs, as the side panes' are: icons with tooltips, the one shown under the
	     active tool's mark. -->
	<div class="flex h-[40px] shrink-0 items-center border-b border-line px-[4px] select-none">
		<div class="tool-group flex items-center gap-0.5" role="tablist" aria-label="Compare panel" {@attach toolMark}>
			<span class="tool-mark" aria-hidden="true" hidden></span>
			{#each TABS as t, i (t.id)}
				<button
					type="button"
					role="tab"
					class="icon-button tool-button"
					aria-selected={compare.panel === t.id}
					aria-controls={t.panel}
					aria-label={t.label}
					data-tip={t.label}
					data-compare-panel-tab={t.id}
					tabindex={compare.panel === t.id ? 0 : -1}
					disabled={t.id === 'differences' && !compared}
					onclick={() => (compare.panel = t.id)}
					onkeydown={(event) => onTabKey(event, i)}
				>
					<t.icon size={18} aria-hidden="true" />
				</button>
			{/each}
		</div>
	</div>

	{#if compare.panel === 'compare'}
		<div id="compare-options" class="flex min-h-0 flex-1 flex-col gap-4 overflow-x-hidden overflow-y-auto p-3" role="tabpanel">
			<div class="flex flex-col gap-1.5">
				<span class="text-sm font-semibold" id="compare-mode-label">Compare pages by</span>
				<Dropdown
					value={compare.settings.mode}
					options={MODES}
					class="h-8 w-full text-[13px]"
					label="Compare pages by"
					disabled={compare.running}
					onchange={(mode) => (compare.settings = { ...compare.settings, mode })}
				/>
			</div>
			<fieldset class="flex flex-col gap-2.5" disabled={compare.running || compare.settings.mode === 'visual'}>
				<legend class="mb-1 text-sm font-semibold">Find</legend>
				<label class="flex items-start gap-2">
					<input
						type="checkbox"
						class="mt-1"
						checked={compare.settings.punctuation}
						onchange={(e) => setOption('punctuation', e.currentTarget.checked)}
					/>
					<span class="flex flex-col">
						<span>Differences in punctuation</span>
						<span class="text-sm text-fg-muted">Commas, full stops, quotes and hyphens</span>
					</span>
				</label>
				<label class="flex items-start gap-2">
					<input
						type="checkbox"
						class="mt-1"
						checked={compare.settings.oneLetter}
						onchange={(e) => setOption('oneLetter', e.currentTarget.checked)}
					/>
					<span class="flex flex-col">
						<span>One-letter differences</span>
						<span class="text-sm text-fg-muted">A word spelled differently by one letter</span>
					</span>
				</label>
				<label class="flex items-start gap-2">
					<input
						type="checkbox"
						class="mt-1"
						checked={compare.settings.margins}
						onchange={(e) => setOption('margins', e.currentTarget.checked)}
					/>
					<span class="flex flex-col">
						<span>Headers and footers</span>
						<span class="text-sm text-fg-muted">Running titles and page numbers, listed apart from the text</span>
					</span>
				</label>
			</fieldset>

			<div class="mt-auto flex flex-col gap-2 border-t border-line pt-3">
				{#if compare.running}
					<ProgressBar value={progress.share === null ? null : progress.share * 100} label="Comparing" />
					<div class="flex items-center gap-2">
						<span class="min-w-0 flex-1 truncate text-sm text-fg-muted" aria-live="polite">
							{compare.stopping ? 'Stopping…' : progress.text}
						</span>
						<button type="button" class="button" disabled={compare.stopping} onclick={() => compare.stop()}>Stop</button>
					</div>
				{:else}
					{#if result}
						<p class="text-sm text-fg-muted">
							{countText(shownCount)} found.
							<button type="button" class="link-button" onclick={() => (compare.panel = 'differences')}>Show them</button>
						</p>
					{:else if !compare.ready}
						<p class="text-sm text-fg-muted">Open both documents to compare them.</p>
					{/if}
					<button
						type="button"
						class="button button-primary w-full"
						disabled={!compare.ready}
						data-compare-run
						onclick={() => void compare.run()}
					>
						{result ? 'Compare again' : 'Compare'}
					</button>
				{/if}
			</div>
		</div>
	{:else}
		<div id="compare-differences" class="flex min-h-0 flex-1 flex-col" role="tabpanel">
			<div class="flex h-[40px] shrink-0 items-center gap-1 border-b border-line px-2" role="toolbar" aria-label="Differences">
				<span class="min-w-0 flex-1 truncate pl-1 text-sm" aria-live="polite">
					{#if result}
						{#if compare.position.at >= 0}
							{(compare.position.at + 1).toLocaleString()} of {compare.position.count.toLocaleString()}
						{:else}
							{countText(shownCount)}
						{/if}
					{:else}
						Not compared yet
					{/if}
				</span>
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
				<button
					type="button"
					class="icon-button tool-button"
					aria-label="Group the differences"
					aria-pressed={compare.grouped}
					data-tip={compare.grouped ? 'Ungroup the differences' : 'Group the differences'}
					disabled={!result}
					onclick={() => (compare.grouped = !compare.grouped)}
				>
					<ListTree size={16} aria-hidden="true" />
				</button>
				<button
					type="button"
					class="icon-button tool-button relative"
					aria-label={compare.kinds.size ? `Filter the differences (${compare.kinds.size} picked)` : 'Filter the differences'}
					data-tip="Filter the differences"
					aria-pressed={filterOpen}
					aria-expanded={filterOpen}
					disabled={!result || result.differences.length === 0}
					onclick={() => (filterOpen = !filterOpen)}
				>
					<ListFilter size={16} aria-hidden="true" />
					{#if compare.kinds.size}
						<span class="filter-dot" aria-hidden="true"></span>
					{/if}
				</button>
				<button
					type="button"
					class="icon-button"
					aria-label="Save the report"
					data-tip="Save the report…"
					disabled={!result}
					onclick={() => void compare.saveReport()}
				>
					<FileText size={16} aria-hidden="true" />
				</button>
			</div>

			{#if filterOpen && result && result.differences.length > 0}
				<div class="flex flex-col gap-2 border-b border-line px-2 py-2" role="group" aria-label="Filters" transition:slide={reveal()}>
					<h3 class="filter-heading" id="compare-filter-kinds">Type</h3>
					<div class="flex flex-wrap items-center gap-0.5" role="group" aria-labelledby="compare-filter-kinds">
						{#each KINDS as kind (kind)}
							{@const Icon = kindIcon(kind)}
							<button
								type="button"
								class="icon-button tool-button"
								aria-label={KIND_NAMES[kind]}
								data-tip={KIND_NAMES[kind]}
								aria-pressed={compare.kinds.has(kind)}
								onclick={() => compare.filter(toggleKind(kind))}
							>
								<Icon size={16} aria-hidden="true" class="text-diff-{kind}" />
							</button>
						{/each}
					</div>
					{#if compare.kinds.size}
						<button type="button" class="button h-7 gap-1 self-start text-xs" onclick={() => compare.filter(new Set())}>
							<X size={14} aria-hidden="true" />Clear filters
						</button>
					{/if}
				</div>
			{/if}

			{#if !result}
				<div class="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center text-fg-muted">
					<p>Compare the documents to see their differences here.</p>
				</div>
			{:else if result.differences.length === 0}
				<div class="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center">
					<p class="font-semibold">No differences found</p>
					<p class="text-sm text-fg-muted">The documents’ text is the same, with the options chosen.</p>
				</div>
			{:else if compare.listed.length === 0 && compare.kinds.size > 0 && !compare.showIgnored}
				<div class="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center">
					<p class="text-sm text-fg-muted">No differences match the filters.</p>
					<button type="button" class="button h-7 gap-1 text-xs" onclick={() => compare.filter(new Set())}>
						<X size={14} aria-hidden="true" />Clear filters
					</button>
				</div>
			{:else}
				<ContextMenu.Root>
					<ContextMenu.Trigger>
						{#snippet child({ props })}
							<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
							<div
								{...props}
								bind:this={list}
								class="compare-list min-h-0 flex-1 overflow-x-hidden overflow-y-auto"
								role="listbox"
								aria-label="Differences"
								aria-multiselectable="true"
								aria-activedescendant={compare.current === null ? undefined : rowId(compare.current)}
								tabindex="0"
								onkeydown={onListKey}
							>
								{#each compare.sections as section (section.title)}
									{#if compare.grouped}
										<div class="compare-section" role="presentation">
											{section.title}
											<span class="text-fg-muted">{section.ids.filter((id) => !compare.ignored.has(id)).length.toLocaleString()}</span>
										</div>
									{/if}
									{#each section.ids as id (id)}
										{@const d = result.differences[id]!}
										{@const Icon = kindIcon(d.kind)}
										{@const ignored = compare.ignored.has(id)}
										<div
											id={rowId(id)}
											class="compare-row"
											class:compare-row-picked={compare.picked.has(id)}
											class:opacity-60={ignored}
											role="option"
											tabindex="-1"
											aria-selected={compare.picked.has(id)}
											aria-label="{KIND_NAMES[d.kind]}{ignored ? ', ignored' : ''}: {sideText(d.old) || '—'} → {sideText(d.new) || '—'}, {pages(d)}"
											data-compare-row={id}
											onpointerdown={(e) => onRowDown(e, id)}
										>
											<span class="compare-kind text-diff-{d.kind}" data-tip={KIND_NAMES[d.kind]}>
												<Icon size={14} aria-hidden="true" />
											</span>
											<span class="flex min-w-0 flex-1 flex-col gap-0.5">
												<span class="compare-text">
													{#if d.kind !== 'inserted'}
														<span class="text-diff-deleted line-through">{sideText(d.old)}</span>
													{/if}
													{#if d.kind === 'changed'}<span class="text-fg-muted"> → </span>{/if}
													{#if d.kind !== 'deleted'}
														<span class="text-diff-inserted">{sideText(d.new)}</span>
													{/if}
												</span>
												<span class="text-sm text-fg-muted">{pages(d)}{ignored ? ' · Ignored' : ''}</span>
											</span>
											{#if ignored}
												<button
													type="button"
													class="icon-button size-6 shrink-0"
													aria-label="Put back in the list"
													data-tip="Put back"
													onpointerdown={(e) => e.stopPropagation()}
													onclick={() => compare.restore([id])}
												>
													<Undo2 size={14} aria-hidden="true" />
												</button>
											{/if}
										</div>
									{/each}
								{/each}
							</div>
						{/snippet}
					</ContextMenu.Trigger>
					<ContextMenu.Portal>
						<ContextMenu.Content class="menu-content">
							{@const id = contextId ?? compare.current}
							{#if id !== null}
								{#if compare.ignored.has(id)}
									<ContextMenu.Item class="menu-item" onSelect={() => compare.restore(targets(id))}>Put back in the list</ContextMenu.Item>
								{:else}
									<ContextMenu.Item class="menu-item" onSelect={() => compare.ignore(targets(id))}>
										{targets(id).length > 1 ? 'Ignore differences' : 'Ignore difference'}<span class="menu-shortcut">Del</span>
									</ContextMenu.Item>
									<ContextMenu.Item class="menu-item" onSelect={() => compare.ignoreIdentical(id)}>Ignore identical differences</ContextMenu.Item>
								{/if}
								<ContextMenu.Separator class="menu-separator" />
								<ContextMenu.Item class="menu-item" onSelect={() => void compare.copy(targets(id))}>
									{targets(id).length > 1 ? 'Copy differences' : 'Copy difference'}<span class="menu-shortcut">Ctrl+C</span>
								</ContextMenu.Item>
							{/if}
						</ContextMenu.Content>
					</ContextMenu.Portal>
				</ContextMenu.Root>
			{/if}

			{#if result && compare.ignored.size > 0}
				<div class="flex h-[36px] shrink-0 items-center gap-2 border-t border-line px-3 text-sm">
					<span class="min-w-0 flex-1 truncate text-fg-muted">{compare.ignored.size.toLocaleString()} ignored</span>
					<button type="button" class="link-button" onclick={() => (compare.showIgnored = !compare.showIgnored)}>
						{compare.showIgnored ? 'Hide them' : 'Show them'}
					</button>
					<button type="button" class="link-button" onclick={() => compare.restore([...compare.ignored])}>Put all back</button>
				</div>
			{/if}
			{#if result && result.differences.length > 0}
				<p class="sr-only" aria-live="polite">
					{counts.inserted} inserted, {counts.deleted} deleted, {counts.changed} changed
				</p>
			{/if}
		</div>
	{/if}
</div>
