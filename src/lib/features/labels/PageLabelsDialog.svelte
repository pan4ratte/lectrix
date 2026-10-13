<script lang="ts">
	// Page labels (section 6.3): a dialog over the grid of pages being organized (section
	// 6.7), as Acrobat's Page Numbering is. It asks which pages (all of them, the ones
	// selected in the grid, or from one page to another) and how they are numbered: a new
	// section with a style, a prefix and a first number (1, 2, 3 from 1 until changed, as
	// Acrobat's "Begin new section"), or the numbering of the page before carried on. The
	// grid's pages show the result under them meanwhile (live preview), and
	// a line of the dialog says it in words. Apply numbers those pages in one undo step;
	// every other page keeps its label.
	import { Dialog } from 'bits-ui';
	import { untrack } from 'svelte';

	import Dropdown from '#lib/components/Dropdown.svelte';
	import type { LabelStyle } from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	import { assign, shownRules } from './actions.ts';
	import {
		STYLES,
		assignLabels,
		labelsForPages,
		numberingProblem,
		pagesText,
		previewText,
		type Numbering
	} from './rules.ts';

	const request = $derived(app.labelsRequest);
	const tab = $derived(request ? (app.tabs.find((t) => t.id === request.tabId) ?? null) : null);
	const count = $derived(tab?.pageCount ?? 0);
	/** The pages selected in the grid, in page order. */
	const selected = $derived(tab?.organize ? tab.organize.indexes(tab.organize.selected) : []);

	let which = $state<'all' | 'selected' | 'range'>('range');
	let from = $state('1');
	let to = $state('1');
	let kind = $state<Numbering['kind']>('new');
	let style = $state<LabelStyle>('decimal');
	let prefix = $state('');
	let first = $state('1');
	let busy = $state(false);

	/** Whole numbers only ("1e3" and "0x10" are not page numbers). */
	function parseWhole(text: string): number {
		return /^\s*\d+\s*$/.test(text) ? Number(text) : Number.NaN;
	}

	// A new request: the pages selected in the grid, else the page with the keyboard focus
	// there; and a new section numbered 1, 2, 3 from 1.
	$effect(() => {
		const current = request;
		if (!current) return;
		untrack(() => {
			const opened = tab;
			if (!opened) return;
			const at = opened.organize?.anchorPage ?? 0;
			which = selected.length === 0 ? 'range' : 'selected';
			from = String(at + 1);
			to = String(at + 1);
			kind = 'new';
			style = 'decimal';
			prefix = '';
			first = '1';
			busy = false;
		});
	});

	/** The pages to number (0-based, sorted), or why there are none. */
	const picked = $derived.by((): { pages: number[]; problem: string | null } => {
		if (which === 'all') return { pages: Array.from({ length: count }, (_, i) => i), problem: null };
		if (which === 'selected') return { pages: selected, problem: null };
		const a = parseWhole(from);
		const b = parseWhole(to);
		if (!(a >= 1 && b >= a && b <= count)) {
			return { pages: [], problem: `Enter pages between 1 and ${count}, the first no later than the last.` };
		}
		return { pages: Array.from({ length: b - a + 1 }, (_, i) => a - 1 + i), problem: null };
	});
	const pages = $derived(picked.pages);
	/** The first page of the document has no page before it to carry on from. */
	const canContinue = $derived(pages[0] !== 0);
	const numbering = $derived<Numbering>(
		kind === 'continue' && canContinue
			? { kind: 'continue' }
			: { kind: 'new', style, prefix, firstNumber: style === 'none' ? 1 : parseWhole(first) }
	);
	const problem = $derived(picked.problem ?? numberingProblem(numbering, pages.length));
	/** Every page's label with the dialog's choices applied, while they can be. */
	const result = $derived(
		tab && pages.length > 0 && !problem
			? labelsForPages(assignLabels(shownRules(tab), count, pages, numbering), count)
			: null
	);

	// Live preview: the grid's pages show the result under them.
	$effect(() => {
		const opened = tab;
		if (!opened) return;
		opened.previewLabels = result;
		return () => {
			opened.previewLabels = null;
		};
	});

	function close() {
		app.labelsRequest = null;
	}

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		if (!tab || busy || problem || pages.length === 0) return;
		busy = true;
		const numbered = [...pages];
		const done = await assign(tab, numbered, numbering);
		busy = false;
		if (!done) return;
		// The first of the pages comes into view in the grid, where its label shows.
		if (which !== 'selected') tab.organize?.show(numbered[0]!, false);
		close();
	}

	const pagesWord = (n: number) => (n === 1 ? '1 page' : `${n.toLocaleString()} pages`);
</script>

<Dialog.Root
	open={request !== null && tab !== null}
	onOpenChange={(open) => {
		if (!open && !busy) close();
	}}
>
	<Dialog.Portal>
		<Dialog.Overlay class="dialog-overlay" />
		<Dialog.Content class="dialog-content w-[440px]" data-labels-dialog>
			{#if request && tab}
				<Dialog.Title class="text-base font-semibold">Page labels</Dialog.Title>
				<Dialog.Description class="mt-1 text-sm text-fg-muted">
					How the pages of {tab.name} are numbered in the page box and in other readers. Pages you don’t pick keep
					their labels.
				</Dialog.Description>
				<form class="mt-4 flex flex-col gap-4" onsubmit={submit}>
					<fieldset class="flex flex-col gap-2">
						<legend class="mb-1 text-sm font-medium">Pages</legend>
						<label class="flex items-center gap-2">
							<input type="radio" bind:group={which} value="all" />
							All {pagesWord(count)}
						</label>
						<label class="flex items-center gap-2" class:opacity-50={selected.length === 0}>
							<input type="radio" bind:group={which} value="selected" disabled={selected.length === 0} />
							{#if selected.length === 0}
								Selected pages (none are selected)
							{:else}
								Selected: {pagesWord(selected.length)} ({pagesText(selected)})
							{/if}
						</label>
						<div class="flex items-center gap-2">
							<label class="flex items-center gap-2">
								<input type="radio" bind:group={which} value="range" />
								From page
							</label>
							<input
								class="field h-7 w-16 tabular-nums"
								inputmode="numeric"
								bind:value={from}
								onfocus={() => (which = 'range')}
								aria-label="First page"
								aria-invalid={which === 'range' && picked.problem !== null}
							/>
							to
							<input
								class="field h-7 w-16 tabular-nums"
								inputmode="numeric"
								bind:value={to}
								onfocus={() => (which = 'range')}
								aria-label="Last page"
								aria-invalid={which === 'range' && picked.problem !== null}
							/>
							<span class="text-fg-muted">of {count.toLocaleString()}</span>
						</div>
					</fieldset>
					<fieldset class="flex flex-col gap-2">
						<legend class="mb-1 text-sm font-medium">Numbering</legend>
						<label class="flex items-center gap-2">
							<input type="radio" bind:group={kind} value="new" />
							Start a new section
						</label>
						<div class="ml-6 grid grid-cols-[auto_1fr] items-center gap-x-3 gap-y-2" class:opacity-50={numbering.kind !== 'new'}>
							<span class="text-sm" aria-hidden="true">Style</span>
							<Dropdown
								class="h-7 w-full"
								bind:value={style}
								label="Style"
								disabled={numbering.kind !== 'new'}
								options={STYLES.map((s) => ({ value: s.value, label: s.name }))}
							/>
							<label class="text-sm" for="labels-prefix">Prefix</label>
							<input
								id="labels-prefix"
								class="field h-7 min-w-0"
								bind:value={prefix}
								disabled={numbering.kind !== 'new'}
								placeholder="None"
							/>
							<label class="text-sm" for="labels-first">Start with</label>
							<input
								id="labels-first"
								class="field h-7 w-24 tabular-nums"
								inputmode="numeric"
								bind:value={first}
								disabled={numbering.kind !== 'new' || style === 'none'}
								data-tip={style === 'none' ? 'This style shows the prefix only.' : undefined}
								aria-invalid={picked.problem === null && problem !== null}
							/>
						</div>
						<label class="flex items-center gap-2" class:opacity-50={!canContinue}>
							<input type="radio" bind:group={kind} value="continue" disabled={!canContinue} />
							Continue the numbering of the page before
						</label>
					</fieldset>
					{#if problem}
						<p class="text-sm text-danger" role="alert">{problem}</p>
					{:else if result}
						<p class="truncate text-sm" data-labels-preview>
							<span class="text-fg-muted">Preview:</span>
							{previewText(result, pages)}
						</p>
					{/if}
					<div class="flex justify-end gap-2">
						<button type="button" class="button" disabled={busy} onclick={close}>Cancel</button>
						<button type="submit" class="button button-primary" disabled={busy || problem !== null || pages.length === 0}>
							Apply
						</button>
					</div>
				</form>
			{/if}
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
