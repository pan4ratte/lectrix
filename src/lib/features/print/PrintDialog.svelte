<script lang="ts">
	// The Print dialog (section 6.12, ADR 0028): the printer and its own settings, copies,
	// the pages, how they are sized and turned on the paper, the paper, both sides,
	// annotations and greys, with a preview of each sheet as it prints. Rust lays the sheets
	// out (`print_layout`) and prints them, with progress and Stop.
	import { ChevronLeft, ChevronRight } from '@lucide/svelte';
	import { Dialog } from 'bits-ui';
	import { untrack } from 'svelte';

	import Dropdown from '#lib/components/Dropdown.svelte';
	import { held } from '#lib/components/held.svelte.ts';
	import ProgressBar from '#lib/components/ProgressBar.svelte';
	import { pageSizeText, usesInches } from '#lib/features/properties/properties.ts';
	import { pageName } from '#lib/features/viewer/pagebox.ts';
	import {
		pageUrl,
		printLayout,
		printPrinter,
		printPrinterSettings,
		printRun,
		printStart,
		printStop,
		toAppError,
		type AppError,
		type PrintDuplex,
		type PrinterDetails,
		type PrinterEntry,
		type PrintLayout,
		type PrintOrientation,
		type PrintProgress,
		type PrintSettings,
		type PrintSizing
	} from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	import {
		DEFAULT_PREFERENCES,
		DUPLEXES,
		ORIENTATIONS,
		SIZINGS,
		SUBSETS,
		initialPrinter,
		pagesText,
		pagesToPrint,
		parseCopies,
		parseScale,
		printerLabel,
		sentMessage,
		type PagesChoice,
		type Subset
	} from './print.ts';

	/** The preview's room, CSS pixels. */
	const PREVIEW = { width: 340, height: 420 };

	const request = $derived(app.printRequest);
	const last = held(() => (request ? (app.tabs.find((t) => t.id === request.tabId) ?? null) : null));
	/** The document printed; null once the dialog closes. */
	const tab = $derived(request ? (app.tabs.find((t) => t.id === request.tabId) ?? null) : null);
	const shown = $derived(last.current);

	let step = $state<'settings' | 'printing'>('settings');
	let printers = $state<PrinterEntry[] | null>(null);
	let printer = $state<string | null>(null);
	let details = $state<PrinterDetails | null>(null);
	/** The printer's settings dialog changed them: the sheets are asked for again. */
	let printerSet = $state(0);
	let problem = $state<AppError | null>(null);

	let copiesText = $state('1');
	let collate = $state(true);
	let choice = $state<PagesChoice>('all');
	let rangeText = $state('');
	let subset = $state<Subset>('all');
	let reverse = $state(false);
	let sizing = $state<PrintSizing>('fit');
	let scaleText = $state('100');
	let orientation = $state<PrintOrientation>('auto');
	let paper = $state<number | null>(null);
	let duplex = $state<PrintDuplex>('oneSided');
	let annotations = $state(true);
	let grey = $state(false);

	let layout = $state<PrintLayout | null>(null);
	let sheet = $state(0);
	let progress = $state<PrintProgress | null>(null);
	/** Each opening of the dialog, printer chosen and layout asked for: what an earlier
	 * one asked is then ignored. */
	let opening = 0;
	let asking = 0;
	let laying = 0;

	const copies = $derived(parseCopies(copiesText));
	const scale = $derived(sizing === 'custom' ? parseScale(scaleText) : 100);
	const current = $derived(shown?.currentPage ?? 0);
	const pages = $derived(
		shown
			? pagesToPrint({ choice, rangeText, current, subset, reverse }, shown.labels, shown.pages.length)
			: null
	);
	const settings = $derived<PrintSettings | null>(
		printer && pages && copies !== null && scale !== null
			? {
					printer,
					pages,
					sizing,
					scale,
					orientation,
					paper,
					duplex: details?.duplex ? duplex : 'oneSided',
					annotations,
					grey,
					copies,
					collate
				}
			: null
	);
	const ready = $derived(settings !== null && details !== null && layout !== null && step === 'settings');

	function close() {
		app.printRequest = null;
	}

	// A new request: start afresh, with the choices used last.
	$effect(() => {
		const current = request;
		if (!current) return;
		untrack(() => void open());
	});

	async function open() {
		const mine = ++opening;
		step = 'settings';
		printers = null;
		printer = null;
		details = null;
		layout = null;
		problem = null;
		progress = null;
		sheet = 0;
		copiesText = '1';
		choice = 'all';
		rangeText = '';
		subset = 'all';
		reverse = false;
		try {
			const start = await printStart();
			if (mine !== opening) return;
			const prefs = start.remembered ?? DEFAULT_PREFERENCES;
			sizing = prefs.sizing;
			scaleText = String(prefs.scale);
			orientation = prefs.orientation;
			duplex = prefs.duplex;
			annotations = prefs.annotations;
			grey = prefs.grey;
			collate = prefs.collate;
			printers = start.printers;
			const first = initialPrinter(start.printers, prefs.printer);
			if (first) await choosePrinter(first);
		} catch (e) {
			if (mine === opening) {
				printers = [];
				problem = toAppError(e);
			}
		}
	}

	async function choosePrinter(name: string) {
		const mine = ++asking;
		printer = name;
		details = null;
		layout = null;
		problem = null;
		try {
			const found = await printPrinter(name);
			if (mine !== asking) return;
			details = found;
			paper = found.paper ?? found.papers[0]?.id ?? null;
			if (!found.duplex) duplex = 'oneSided';
		} catch (e) {
			if (mine === asking) problem = toAppError(e);
		}
	}

	async function printerSettings() {
		if (!printer) return;
		const name = printer;
		try {
			const found = await printPrinterSettings(name);
			if (!found || printer !== name) return;
			details = found;
			paper = found.paper ?? paper;
			// Both sides as the printer's own settings now say.
			if (!found.duplex || !found.duplexOn) duplex = 'oneSided';
			else if (duplex === 'oneSided') duplex = 'longEdge';
			printerSet++;
		} catch (e) {
			problem = toAppError(e);
		}
	}

	// The sheets, laid out again a moment after what places them changes.
	$effect(() => {
		const doc = tab;
		const wanted = settings;
		const known = details;
		void printerSet;
		if (!doc || !wanted || !known || step !== 'settings') return;
		const mine = ++laying;
		const timer = setTimeout(async () => {
			try {
				const found = await printLayout(doc.id, $state.snapshot(wanted) as PrintSettings);
				if (mine !== laying) return;
				layout = found;
				problem = null;
				sheet = Math.min(sheet, Math.max(0, found.sheets.length - 1));
			} catch (e) {
				if (mine === laying) {
					layout = null;
					problem = toAppError(e);
				}
			}
		}, 120);
		return () => clearTimeout(timer);
	});

	// The preview starts on the page shown, where it prints.
	let placedFor = 0;
	$effect(() => {
		const found = layout;
		if (!found || placedFor === opening) return;
		placedFor = opening;
		const at = found.sheets.findIndex((s) => s.page === untrack(() => current));
		sheet = Math.max(0, at);
	});

	async function print(event?: SubmitEvent) {
		event?.preventDefault();
		const doc = tab;
		const wanted = settings;
		if (!doc || !wanted || !ready) return;
		const name = printerLabel(wanted.printer, false);
		step = 'printing';
		progress = null;
		try {
			const done = await printRun(doc.id, $state.snapshot(wanted) as PrintSettings, (p) => (progress = p));
			close();
			app.notify({ kind: 'info', message: sentMessage(doc.name, name, done.pages) });
		} catch (e) {
			const error = toAppError(e);
			close();
			if (error.code === 'cancelled') app.notify({ kind: 'info', message: error.message });
			else app.showError(error);
		}
	}

	function stop() {
		if (tab) void printStop(tab.id);
	}

	// ----- the preview -----

	const placed = $derived(layout?.sheets[sheet] ?? null);
	const k = $derived(layout ? Math.min(PREVIEW.width / layout.width, PREVIEW.height / layout.height) : 1);
	const ratio = typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1;
	/** The page's box before it is turned, CSS pixels. */
	const box = $derived(
		placed
			? placed.turn !== 'upright'
				? { width: placed.area.height * k, height: placed.area.width * k }
				: { width: placed.area.width * k, height: placed.area.height * k }
			: null
	);
	const imageUrl = $derived.by(() => {
		const doc = shown;
		if (!doc || !placed || !box) return null;
		const size = doc.pages[placed.page];
		if (!size) return null;
		const scale = Math.min(4, (box.width / size.width) * ratio);
		return pageUrl(doc.id, placed.page, scale, doc.state.revision, undefined, 'png', [], annotations ? 'print' : 'print-content');
	});
	const paperText = $derived.by(() => {
		if (!layout || !details) return '';
		const named = details.papers.find((p) => p.id === paper)?.name;
		const size = pageSizeText(layout.width, layout.height, usesInches(navigator.language));
		return named ? `${named}, ${size.replace(/ \(.*\)$/, '')}` : size;
	});
	const sheetTotal = $derived(layout?.sheets.length ?? 0);
</script>

<Dialog.Root
	open={request !== null}
	onOpenChange={(open) => {
		if (open) return;
		// Esc while printing stops it; otherwise the dialog closes, printing nothing.
		if (step === 'printing') stop();
		else close();
	}}
>
	<Dialog.Portal>
		<Dialog.Overlay class="dialog-overlay" />
		<Dialog.Content
			class="dialog-content flex max-h-[calc(100vh-48px)] w-[820px] flex-col"
			data-print-dialog
			data-step={step}
			interactOutsideBehavior="ignore"
		>
			{#if shown}
				<Dialog.Title class="truncate text-base font-semibold">Print {shown.name}</Dialog.Title>
				<Dialog.Description class="mt-1 text-sm text-fg-muted">
					{pagesText(shown.pages.length)}{shown.state.dirty ? ', with its unsaved changes' : ''}
				</Dialog.Description>

				{#if step === 'printing'}
					<div class="mt-6 flex flex-col gap-2" data-print-progress>
						<ProgressBar value={progress ? progress.done : null} max={progress?.total || 1} label="Printing" />
						<p class="text-sm text-fg-muted" aria-live="polite">
							{progress && progress.total > 0
								? `Sending page ${Math.min(progress.done + 1, progress.total).toLocaleString()} of ${progress.total.toLocaleString()} to the printer…`
								: 'Getting the printer ready…'}
						</p>
					</div>
					<div class="mt-6 flex justify-end">
						<button type="button" class="button" onclick={stop} data-print-stop>Stop</button>
					</div>
				{:else}
					<form class="mt-4 flex min-h-0 flex-col" onsubmit={print}>
						<div class="-mx-6 flex min-h-0 gap-6 overflow-y-auto overflow-x-hidden px-6">
							<div class="flex w-[380px] shrink-0 flex-col gap-4" data-print-settings>
								<div class="flex flex-col gap-2">
									<div class="flex items-center gap-2">
										<span class="w-[88px] shrink-0 font-medium" aria-hidden="true">Printer</span>
										{#if printers === null}
											<span class="text-sm text-fg-muted">Looking for printers…</span>
										{:else if printers.length === 0}
											<span class="text-sm text-fg-muted">No printers</span>
										{:else}
											<Dropdown
												class="h-8 min-w-0 flex-1"
												value={printer ?? ''}
												label="Printer"
												options={printers.map((p) => ({ value: p.name, label: printerLabel(p.name, p.default) }))}
												onchange={(name) => void choosePrinter(name)}
											/>
										{/if}
									</div>
									{#if details?.settingsDialog}
										<button
											type="button"
											class="button ml-[96px] h-7 self-start text-sm"
											data-tip="This printer’s own settings: its paper trays, print quality and finishing"
											onclick={printerSettings}
										>
											Printer settings…
										</button>
									{/if}
									{#if printers !== null && printers.length === 0 && !problem}
										<p class="text-sm text-fg-muted">
											No printer is set up on this computer. Add one in the system’s settings, then open this dialog again.
										</p>
									{/if}
								</div>

								<div class="flex items-center gap-2">
									<label for="print-copies" class="w-[88px] shrink-0 font-medium">Copies</label>
									<input
										id="print-copies"
										class="field h-7 w-16 tabular-nums"
										inputmode="numeric"
										bind:value={copiesText}
										aria-invalid={copies === null}
									/>
									<label class="ml-3 flex items-center gap-2 text-sm" class:opacity-50={copies === null || copies < 2}>
										<input type="checkbox" bind:checked={collate} disabled={copies === null || copies < 2} />
										Collate
									</label>
								</div>

								<fieldset class="flex flex-col gap-2">
									<legend class="mb-1 font-medium">Pages</legend>
									<label class="flex items-center gap-2">
										<input type="radio" bind:group={choice} value="all" />
										All {pagesText(shown.pages.length)}
									</label>
									<label class="flex items-center gap-2">
										<input type="radio" bind:group={choice} value="current" />
										The current page ({pageName(current, shown.labels)})
									</label>
									<label class="flex items-center gap-2">
										<input type="radio" bind:group={choice} value="range" />
										Pages
										<input
											class="field h-7 min-w-0 flex-1"
											bind:value={rangeText}
											onfocus={() => (choice = 'range')}
											placeholder={shown.labels ? 'i-iv, 1-5, 8' : '1-5, 8, 11-13'}
											aria-label="Pages to print"
											aria-invalid={choice === 'range' && rangeText.trim() !== '' && pages === null}
											data-print-range
										/>
									</label>
									{#if choice === 'range' && rangeText.trim() !== '' && pages === null}
										<p class="ml-6 text-sm text-danger" role="alert">
											Type pages as {shown.labels ? 'they are numbered, as i-iv, 1-5, 8' : '1-5, 8, 11-13'}, up to {shown.pages.length}.
										</p>
									{/if}
									<div class="ml-6 flex items-center gap-2">
										<Dropdown class="h-7 w-[180px]" bind:value={subset} label="Which of these pages" options={SUBSETS} />
										<label class="ml-2 flex items-center gap-2 text-sm">
											<input type="checkbox" bind:checked={reverse} />
											Last page first
										</label>
									</div>
								</fieldset>

								<fieldset class="flex flex-col gap-2">
									<legend class="mb-1 font-medium">Layout</legend>
									<div class="flex items-center gap-2">
										<span class="w-[88px] shrink-0 text-sm" aria-hidden="true">Size</span>
										<Dropdown class="h-7 min-w-0 flex-1" bind:value={sizing} label="Page size on the paper" options={SIZINGS} />
									</div>
									{#if sizing === 'custom'}
										<div class="ml-[96px] flex items-center gap-2">
											<input
												class="field h-7 w-16 tabular-nums"
												inputmode="numeric"
												bind:value={scaleText}
												aria-label="Scale, in percent"
												aria-invalid={scale === null}
											/>
											<span class="text-sm">%</span>
										</div>
									{/if}
									<div class="flex items-center gap-2">
										<span class="w-[88px] shrink-0 text-sm" aria-hidden="true">Orientation</span>
										<Dropdown class="h-7 min-w-0 flex-1" bind:value={orientation} label="Orientation" options={ORIENTATIONS} />
									</div>
									{#if details && details.papers.length > 0}
										<div class="flex items-center gap-2">
											<span class="w-[88px] shrink-0 text-sm" aria-hidden="true">Paper</span>
											<Dropdown
												class="h-7 min-w-0 flex-1"
												value={paper ?? details.papers[0]?.id ?? 0}
												label="Paper"
												options={details.papers.map((p) => ({ value: p.id, label: p.name }))}
												onchange={(id) => (paper = id)}
											/>
										</div>
									{/if}
									{#if details?.duplex}
										<div class="flex items-center gap-2">
											<span class="w-[88px] shrink-0 text-sm" aria-hidden="true">Sides</span>
											<Dropdown class="h-7 min-w-0 flex-1" bind:value={duplex} label="Print on" options={DUPLEXES} />
										</div>
									{/if}
								</fieldset>

								<fieldset class="flex flex-col gap-2">
									<legend class="mb-1 font-medium">Print</legend>
									<label class="flex items-center gap-2">
										<input type="checkbox" bind:checked={annotations} data-print-annotations />
										Annotations
									</label>
									<label class="flex items-center gap-2">
										<input type="checkbox" bind:checked={grey} />
										In greys (black and white)
									</label>
								</fieldset>

								{#if layout?.lowResolution}
									<p class="text-sm text-fg-muted">
										This document allows printing at low resolution only, so its pages print at 150 dpi.
									</p>
								{/if}
								{#if problem}
									<p class="text-sm text-danger" role="alert">
										{problem.message}{problem.suggestion ? ` ${problem.suggestion}` : ''}
									</p>
								{/if}
							</div>

							<div class="flex min-w-0 flex-1 flex-col items-center gap-2" data-print-preview>
								<div
									class="flex items-center justify-center rounded-[6px] bg-canvas"
									style:width="{PREVIEW.width + 24}px"
									style:height="{PREVIEW.height + 24}px"
								>
									{#if layout && placed && box}
										<div
											class="relative bg-white shadow-[0_1px_4px_var(--color-page-shadow)]"
											style:width="{layout.width * k}px"
											style:height="{layout.height * k}px"
											role="img"
											aria-label="Sheet {sheet + 1}: page {pageName(placed.page, shown.labels)}"
										>
											<div
												class="absolute overflow-hidden"
												style:left="{layout.printable.x * k}px"
												style:top="{layout.printable.y * k}px"
												style:width="{layout.printable.width * k}px"
												style:height="{layout.printable.height * k}px"
											>
												{#if imageUrl}
													<img
														src={imageUrl}
														alt=""
														class="absolute max-w-none"
														class:grayscale={grey}
														style:width="{box.width}px"
														style:height="{box.height}px"
														style:left="{(placed.area.x - layout.printable.x + placed.area.width / 2) * k - box.width / 2}px"
														style:top="{(placed.area.y - layout.printable.y + placed.area.height / 2) * k - box.height / 2}px"
														style:transform={placed.turn === 'left' ? 'rotate(-90deg)' : placed.turn === 'right' ? 'rotate(90deg)' : undefined}
														data-print-page={placed.page}
													/>
												{/if}
											</div>
										</div>
									{:else if problem}
										<span class="px-6 text-center text-sm text-fg-muted">No preview</span>
									{:else}
										<span class="text-sm text-fg-muted">Preparing the preview…</span>
									{/if}
								</div>
								<div class="flex items-center gap-2">
									<button
										type="button"
										class="icon-button"
										data-tip="Previous page"
										aria-label="Previous page"
										disabled={sheet <= 0 || !layout}
										onclick={() => (sheet = Math.max(0, sheet - 1))}
									>
										<ChevronLeft size={16} aria-hidden="true" />
									</button>
									<span class="text-sm tabular-nums" data-print-sheet>
										{sheetTotal > 0 ? `${(sheet + 1).toLocaleString()} of ${sheetTotal.toLocaleString()}` : '–'}
									</span>
									<button
										type="button"
										class="icon-button"
										data-tip="Next page"
										aria-label="Next page"
										disabled={sheet >= sheetTotal - 1 || !layout}
										onclick={() => (sheet = Math.min(sheetTotal - 1, sheet + 1))}
									>
										<ChevronRight size={16} aria-hidden="true" />
									</button>
								</div>
								<p class="text-xs text-fg-muted" data-print-paper>{paperText}</p>
							</div>
						</div>
						<div class="mt-5 flex justify-end gap-2">
							<button type="button" class="button" onclick={close}>Cancel</button>
							<button type="submit" class="button button-primary" disabled={!ready} data-print-run>Print</button>
						</div>
					</form>
				{/if}
			{/if}
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
