<script lang="ts">
	// The Export as images dialog (section 6.14, ADR 0034): the pages, the format and its
	// options, the resolution, colours, annotations and how files are named, with a preview
	// of a page as it will be written. Export… asks where in the Save dialog (Rust), then,
	// if some files are there already, whether to replace them; the export shows progress
	// and Stop.
	import { ChevronLeft, ChevronRight } from '@lucide/svelte';
	import { Dialog } from 'bits-ui';
	import { untrack } from 'svelte';

	import Dropdown from '#lib/components/Dropdown.svelte';
	import { held } from '#lib/components/held.svelte.ts';
	import ProgressBar from '#lib/components/ProgressBar.svelte';
	import Segmented from '#lib/components/Segmented.svelte';
	import Slider from '#lib/components/Slider.svelte';
	import { pageName } from '#lib/features/viewer/pagebox.ts';
	import {
		exportForget,
		exportPick,
		exportPreview,
		exportPreviewUrl,
		exportRun,
		exportShowInFolder,
		exportStart,
		exportStop,
		toAppError,
		type AppError,
		type ExportFormat,
		type ExportPreview,
		type ExportProgress,
		type ExportSettings,
		type ExportTarget,
		type FileNaming,
		type ImageColour,
		type TiffCompression
	} from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	import {
		DEFAULT_PREFERENCES,
		DPI_PRESETS,
		FORMAT_NOTES,
		FORMATS,
		SHOW_IN_FOLDER,
		TIFF_COMPRESSIONS,
		colours,
		doneMessage,
		exampleName,
		existingText,
		hasQuality,
		pagesText,
		pagesToExport,
		parseDpi,
		pixelSize,
		previewSizeText,
		stemOf,
		takesBlackAndWhite,
		tooLarge,
		tooLargeText,
		type PagesChoice
	} from './export.ts';

	/** The preview's room, CSS pixels. */
	const PREVIEW = { width: 340, height: 400 };

	const request = $derived(app.exportRequest);
	const last = held(() => (request ? (app.tabs.find((t) => t.id === request.tabId) ?? null) : null));
	/** The document exported; null once the dialog closes. */
	const tab = $derived(request ? (app.tabs.find((t) => t.id === request.tabId) ?? null) : null);
	const shown = $derived(last.current);
	/** The pages selected where the dialog was opened (Organize pages). */
	const selected = $derived(request?.selected ?? null);

	let step = $state<'settings' | 'confirm' | 'exporting'>('settings');
	let problem = $state<AppError | null>(null);

	let choice = $state<PagesChoice>('all');
	let rangeText = $state('');
	/** The page the dialog was opened for, or the one shown. */
	let current = $state(0);
	let format = $state<ExportFormat>('png');
	let dpiChoice = $state<string>('150');
	let dpiText = $state('150');
	let colour = $state<ImageColour>('colour');
	let quality = $state(85);
	let tiff = $state<TiffCompression>('lzw');
	let oneFile = $state(false);
	let naming = $state<FileNaming>('numbers');
	let annotations = $state(true);
	let ready = $state(false);

	let preview = $state<ExportPreview | null>(null);
	let previewing = $state(false);
	let previewProblem = $state<AppError | null>(null);
	/** Which of the pages exported the preview shows. */
	let previewAt = $state(0);
	let previewScale = $state<'fit' | 'actual'>('fit');
	let target = $state<ExportTarget | null>(null);
	let progress = $state<ExportProgress | null>(null);
	/** Each opening of the dialog, and each preview asked for: what an earlier one asked is
	 * then ignored. */
	let opening = 0;
	let asking = 0;

	const labels = $derived(shown?.labels ?? null);
	const count = $derived(shown?.pages.length ?? 0);
	const dpi = $derived(dpiChoice === 'custom' ? parseDpi(dpiText) : Number(dpiChoice));
	const pages = $derived(
		shown ? pagesToExport({ choice, rangeText, current, selected: selected ?? [] }, labels, count) : null
	);
	/** Black and white only where the format keeps it. */
	const colourUsed = $derived<ImageColour>(colour === 'blackAndWhite' && !takesBlackAndWhite(format) ? 'grey' : colour);
	const oneTiff = $derived(format === 'tiff' && oneFile && (pages?.length ?? 0) > 1);
	const fileCount = $derived(oneTiff ? 1 : new Set(pages ?? []).size);
	const large = $derived(shown && pages && dpi !== null ? tooLarge(format, dpi, pages, shown.pages) : null);
	const settings = $derived<ExportSettings | null>(
		pages && dpi !== null
			? {
					pages,
					format,
					dpi,
					colour: colourUsed,
					quality,
					tiff,
					oneFile: oneTiff,
					naming,
					annotations
				}
			: null
	);
	const canExport = $derived(ready && settings !== null && large === null && step === 'settings');

	function close() {
		if (tab) void exportForget(tab.id);
		app.exportRequest = null;
	}

	// A new request: start afresh, with the choices used last.
	$effect(() => {
		const current = request;
		if (!current) return;
		untrack(() => void open());
	});

	async function open() {
		const mine = ++opening;
		const r = request;
		const doc = tab;
		if (!r || !doc) return;
		step = 'settings';
		problem = null;
		preview = null;
		previewProblem = null;
		target = null;
		progress = null;
		previewAt = 0;
		ready = false;
		current = r.page ?? doc.currentPage;
		choice = r.selected ? 'selected' : r.page !== null ? 'current' : 'all';
		rangeText = '';
		try {
			const prefs = (await exportStart()) ?? DEFAULT_PREFERENCES;
			if (mine !== opening) return;
			format = prefs.format;
			const preset = DPI_PRESETS.find((d) => d === prefs.dpi);
			dpiChoice = preset ? String(preset) : 'custom';
			dpiText = String(prefs.dpi);
			colour = prefs.colour;
			quality = prefs.quality;
			tiff = prefs.tiff;
			oneFile = prefs.oneFile;
			naming = prefs.naming;
			annotations = prefs.annotations;
		} catch (e) {
			if (mine === opening) problem = toAppError(e);
		}
		if (mine === opening) ready = true;
	}

	// The preview starts on the page the dialog was opened for, where it is exported.
	let placedFor = 0;
	$effect(() => {
		const found = pages;
		if (!found || placedFor === opening || !ready) return;
		placedFor = opening;
		previewAt = Math.max(0, found.indexOf(untrack(() => current)));
	});

	const previewPage = $derived(pages ? (pages[Math.min(previewAt, pages.length - 1)] ?? null) : null);

	// The preview, made again a moment after what it shows changes.
	$effect(() => {
		const doc = tab;
		const wanted = settings;
		const page = previewPage;
		if (!doc || !wanted || page === null || !ready || step !== 'settings') return;
		if (large) {
			preview = null;
			previewing = false;
			return;
		}
		const mine = ++asking;
		previewing = true;
		const timer = setTimeout(async () => {
			try {
				const found = await exportPreview(doc.id, page, $state.snapshot(wanted) as ExportSettings);
				if (mine !== asking) return;
				preview = found;
				previewProblem = null;
			} catch (e) {
				const error = toAppError(e);
				if (mine !== asking || error.code === 'cancelled') return;
				preview = null;
				previewProblem = error;
			} finally {
				if (mine === asking) previewing = false;
			}
		}, 200);
		return () => clearTimeout(timer);
	});

	async function pick(event?: SubmitEvent) {
		event?.preventDefault();
		const doc = tab;
		const wanted = settings;
		if (!doc || !wanted || !canExport) return;
		problem = null;
		try {
			const found = await exportPick(doc.id, $state.snapshot(wanted) as ExportSettings);
			if (!found) return;
			target = found;
			if (found.existingCount > 0) step = 'confirm';
			else await run(false);
		} catch (e) {
			problem = toAppError(e);
		}
	}

	async function run(skipExisting: boolean) {
		const doc = tab;
		if (!doc) return;
		const name = doc.name;
		const exported = format;
		step = 'exporting';
		progress = null;
		try {
			const done = await exportRun(doc.id, skipExisting, (p) => (progress = p));
			app.exportRequest = null;
			app.notify({
				kind: 'info',
				message: doneMessage(name, done, exported),
				action: done.files > 0 ? { label: SHOW_IN_FOLDER, run: () => void showFolder(done.token) } : undefined
			});
		} catch (e) {
			const error = toAppError(e);
			app.exportRequest = null;
			if (error.code === 'cancelled') app.notify({ kind: 'info', message: error.message });
			else app.showError(error);
		}
	}

	async function showFolder(token: number) {
		try {
			await exportShowInFolder(token);
		} catch (e) {
			app.showError(toAppError(e));
		}
	}

	function stop() {
		if (tab) void exportStop(tab.id);
	}

	// ----- what the dialog shows -----

	const dpiOptions = [
		...DPI_PRESETS.map((d) => ({ value: String(d), label: `${d} dpi` })),
		{ value: 'custom', label: 'Custom' }
	];
	const pixels = $derived.by(() => {
		const size = previewPage !== null ? shown?.pages[previewPage] : undefined;
		return size && dpi !== null ? pixelSize(size.width, size.height, dpi) : null;
	});
	const estimated = $derived(preview !== null && dpi !== null && preview.dpi < dpi);
	const previewUrl = $derived(preview ? exportPreviewUrl(preview.token) : null);
	const ratio = typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1;
	/** The preview image's size on screen, CSS pixels. */
	const box = $derived.by(() => {
		if (!preview) return null;
		if (previewScale === 'actual') {
			// One pixel of the file to one of the screen, at the resolution asked for.
			const k = dpi !== null ? dpi / preview.dpi : 1;
			return { width: (preview.width * k) / ratio, height: (preview.height * k) / ratio };
		}
		const k = Math.min(PREVIEW.width / preview.width, PREVIEW.height / preview.height);
		return { width: preview.width * k, height: preview.height * k };
	});
	const stem = $derived(stemOf(shown?.name ?? 'Document'));
	const example = $derived(
		pages && pages.length > 0 ? exampleName(stem, pages[0]!, count, naming, labels, format) : ''
	);
	const pagesTotal = $derived(pages?.length ?? 0);
</script>

<Dialog.Root
	open={request !== null}
	onOpenChange={(open) => {
		if (open) return;
		// Esc while exporting stops it; otherwise the dialog closes, exporting nothing.
		if (step === 'exporting') stop();
		else if (step === 'confirm') step = 'settings';
		else close();
	}}
>
	<Dialog.Portal>
		<Dialog.Overlay class="dialog-overlay" />
		<Dialog.Content
			class="dialog-content flex max-h-[calc(100vh-48px)] w-[820px] flex-col"
			data-export-dialog
			data-step={step}
			interactOutsideBehavior="ignore"
		>
			{#if shown}
				<Dialog.Title class="truncate text-base font-semibold">Export {shown.name} as images</Dialog.Title>
				<Dialog.Description class="mt-1 text-sm text-fg-muted">
					{pagesText(count)}{shown.state.dirty ? ', with its unsaved changes' : ''}
				</Dialog.Description>

				{#if step === 'exporting'}
					<div class="mt-6 flex flex-col gap-2" data-export-progress>
						<ProgressBar value={progress ? progress.done : null} max={progress?.total || 1} label="Exporting" />
						<p class="text-sm text-fg-muted" aria-live="polite">
							{progress && progress.total > 0
								? `Exporting page ${Math.min(progress.done + 1, progress.total).toLocaleString()} of ${progress.total.toLocaleString()}…`
								: 'Getting the pages ready…'}
						</p>
					</div>
					<div class="mt-6 flex justify-end">
						<button type="button" class="button" onclick={stop} data-export-stop>Stop</button>
					</div>
				{:else if step === 'confirm' && target}
					<div class="mt-5 flex flex-col gap-2" data-export-confirm>
						<p>{existingText(target)}</p>
						<p class="text-sm text-fg-muted">
							Replace them with the pages exported now, or keep them and export only the pages whose files aren’t there.
						</p>
					</div>
					<div class="mt-6 flex justify-end gap-2">
						<button type="button" class="button" onclick={() => (step = 'settings')}>Back</button>
						<button type="button" class="button" onclick={() => void run(true)} data-export-skip>Keep them</button>
						<button type="button" class="button button-primary" onclick={() => void run(false)} data-export-replace>
							Replace
						</button>
					</div>
				{:else}
					<form class="mt-4 flex min-h-0 flex-col" onsubmit={pick}>
						<div class="-mx-6 flex min-h-0 gap-6 overflow-y-auto overflow-x-hidden px-6">
							<div class="flex w-[380px] shrink-0 flex-col gap-4" data-export-settings>
								<fieldset class="flex flex-col gap-2">
									<legend class="mb-1 font-medium">Pages</legend>
									<label class="flex items-center gap-2">
										<input type="radio" bind:group={choice} value="all" />
										All {pagesText(count)}
									</label>
									<label class="flex items-center gap-2">
										<input type="radio" bind:group={choice} value="current" />
										{request?.page !== null && request?.page !== undefined ? 'This page' : 'The current page'} ({pageName(current, labels)})
									</label>
									{#if selected}
										<label class="flex items-center gap-2">
											<input type="radio" bind:group={choice} value="selected" />
											The {selected.length === 1 ? 'selected page' : `${selected.length.toLocaleString()} selected pages`}
										</label>
									{/if}
									<label class="flex items-center gap-2">
										<input type="radio" bind:group={choice} value="range" />
										Pages
										<input
											class="field h-7 min-w-0 flex-1"
											bind:value={rangeText}
											onfocus={() => (choice = 'range')}
											placeholder={labels ? 'i-iv, 1-5, 8' : '1-5, 8, 11-13'}
											aria-label="Pages to export"
											aria-invalid={choice === 'range' && rangeText.trim() !== '' && pages === null}
											data-export-range
										/>
									</label>
									{#if choice === 'range' && rangeText.trim() !== '' && pages === null}
										<p class="ml-6 text-sm text-danger" role="alert">
											Type pages as {labels ? 'they are numbered, as i-iv, 1-5, 8' : '1-5, 8, 11-13'}, up to {count}.
										</p>
									{/if}
								</fieldset>

								<fieldset class="flex flex-col gap-2">
									<legend class="mb-1 font-medium">Image</legend>
									<div class="flex items-center gap-2">
										<span class="w-[88px] shrink-0 text-sm" aria-hidden="true">Format</span>
										<Dropdown
											class="h-7 min-w-0 flex-1"
											value={format}
											label="Format"
											options={FORMATS}
											onchange={(f) => (format = f)}
										/>
									</div>
									<p class="ml-[96px] text-xs text-fg-muted" data-export-format-note>{FORMAT_NOTES[format]}</p>
									<div class="flex items-center gap-2">
										<span class="w-[88px] shrink-0 text-sm" aria-hidden="true">Resolution</span>
										<Dropdown
											class="h-7 w-[120px]"
											value={dpiChoice}
											label="Resolution"
											options={dpiOptions}
											onchange={(d) => {
												dpiChoice = d;
												if (d !== 'custom') dpiText = d;
											}}
										/>
										{#if dpiChoice === 'custom'}
											<input
												class="field h-7 w-16 tabular-nums"
												inputmode="numeric"
												bind:value={dpiText}
												aria-label="Resolution, in dots per inch"
												aria-invalid={dpi === null}
												data-export-dpi
											/>
											<span class="text-sm">dpi</span>
										{/if}
									</div>
									{#if dpiChoice === 'custom' && dpi === null}
										<p class="ml-[96px] text-sm text-danger" role="alert">Type a resolution from 10 to 2,400 dpi.</p>
									{/if}
									{#if large}
										<p class="ml-[96px] text-sm text-danger" role="alert" data-export-too-large>
											{tooLargeText(large, format, labels)}
										</p>
									{/if}
									<div class="flex items-center gap-2">
										<span class="w-[88px] shrink-0 text-sm" aria-hidden="true">Colour</span>
										<Dropdown
											class="h-7 min-w-0 flex-1"
											value={colourUsed}
											label="Colour"
											options={colours(format)}
											onchange={(c) => (colour = c)}
										/>
									</div>
									{#if hasQuality(format)}
										<div class="flex items-center gap-2">
											<span class="w-[88px] shrink-0 text-sm" aria-hidden="true">Quality</span>
											<span class="text-xs text-fg-muted" aria-hidden="true">Low</span>
											<div class="w-[150px]">
												<Slider
													bind:value={quality}
													min={1}
													max={100}
													step={1}
													label="Quality"
													valuetext={String(quality)}
												/>
											</div>
											<span class="text-xs text-fg-muted" aria-hidden="true">High</span>
											<span class="w-[28px] text-right text-sm tabular-nums" aria-hidden="true">{quality}</span>
										</div>
									{/if}
									{#if format === 'tiff'}
										<div class="flex items-center gap-2">
											<span class="w-[88px] shrink-0 text-sm" aria-hidden="true">Compression</span>
											{#if colourUsed === 'blackAndWhite'}
												<span class="text-sm">CCITT Group 4</span>
											{:else}
												<Dropdown
													class="h-7 w-[120px]"
													value={tiff}
													label="TIFF compression"
													options={TIFF_COMPRESSIONS}
													onchange={(t) => (tiff = t)}
												/>
											{/if}
										</div>
										{#if (pages?.length ?? 0) > 1}
											<label class="ml-[96px] flex items-center gap-2 text-sm">
												<input type="checkbox" bind:checked={oneFile} data-export-one-file />
												All pages in one file
											</label>
										{/if}
									{/if}
									<label class="flex items-center gap-2">
										<input type="checkbox" bind:checked={annotations} data-export-annotations />
										Annotations
									</label>
								</fieldset>

								{#if fileCount > 1}
									<fieldset class="flex flex-col gap-2">
										<legend class="mb-1 font-medium">Files</legend>
										{#if labels}
											<div class="flex items-center gap-2">
												<span id="export-naming" class="w-[88px] shrink-0 text-sm">Name by</span>
												<Segmented
													value={naming}
													labelledby="export-naming"
													options={[
														{ value: 'numbers', label: 'Page numbers' },
														{ value: 'labels', label: 'Page labels' }
													]}
													onchange={(n) => (naming = n as FileNaming)}
												/>
											</div>
										{/if}
										<p class="text-sm text-fg-muted" data-export-naming-example>
											A file a page, named after the name you give: {example}, and so on.
										</p>
									</fieldset>
								{/if}

								{#if problem}
									<p class="text-sm text-danger" role="alert">
										{problem.message}{problem.suggestion ? ` ${problem.suggestion}` : ''}
									</p>
								{/if}
							</div>

							<div class="flex min-w-0 flex-1 flex-col items-center gap-2" data-export-preview>
								<div
									class={[
										'relative flex overflow-auto rounded-[6px] bg-canvas',
										previewScale === 'actual' && preview ? 'items-start justify-start' : 'items-center justify-center'
									]}
									style:width="{PREVIEW.width + 24}px"
									style:height="{PREVIEW.height + 24}px"
									data-export-preview-pane
								>
									{#if previewUrl && box}
										<img
											src={previewUrl}
											alt="Page {previewPage !== null ? pageName(previewPage, labels) : ''} as it will be exported"
											class="max-w-none shrink-0 bg-white shadow-[0_1px_4px_var(--color-page-shadow)]"
											class:m-3={previewScale === 'actual'}
											style:width="{box.width}px"
											style:height="{box.height}px"
											style:image-rendering={previewScale === 'actual' ? 'pixelated' : undefined}
											data-export-preview-image
										/>
									{:else if large}
										<span class="px-6 text-center text-sm text-fg-muted">No preview</span>
									{:else if previewProblem}
										<span class="px-6 text-center text-sm text-fg-muted">
											{previewProblem.message}
										</span>
									{:else}
										<span class="text-sm text-fg-muted">Preparing the preview…</span>
									{/if}
									{#if previewing && preview}
										<span
											class="absolute right-2 top-2 rounded-[4px] bg-surface px-2 py-0.5 text-xs text-fg-muted shadow-sm"
											aria-live="polite">Updating…</span
										>
									{/if}
								</div>
								<div class="flex items-center gap-2">
									<button
										type="button"
										class="icon-button"
										data-tip="Previous page"
										aria-label="Previous page"
										disabled={previewAt <= 0 || !pages}
										onclick={() => (previewAt = Math.max(0, previewAt - 1))}
									>
										<ChevronLeft size={16} aria-hidden="true" />
									</button>
									<span class="text-sm tabular-nums" data-export-preview-page>
										{pagesTotal > 0 ? `${(Math.min(previewAt, pagesTotal - 1) + 1).toLocaleString()} of ${pagesTotal.toLocaleString()}` : '–'}
									</span>
									<button
										type="button"
										class="icon-button"
										data-tip="Next page"
										aria-label="Next page"
										disabled={previewAt >= pagesTotal - 1 || !pages}
										onclick={() => (previewAt = Math.min(pagesTotal - 1, previewAt + 1))}
									>
										<ChevronRight size={16} aria-hidden="true" />
									</button>
									<span class="ml-2">
										<Segmented
											value={previewScale}
											options={[
												{ value: 'fit', label: 'Whole page' },
												{ value: 'actual', label: 'Actual pixels' }
											]}
											onchange={(v) => (previewScale = v as 'fit' | 'actual')}
										/>
									</span>
								</div>
								<p class="text-xs text-fg-muted tabular-nums" data-export-preview-size>
									{#if pixels && preview && !large}
										{pixels.width.toLocaleString()} × {pixels.height.toLocaleString()} pixels · {previewSizeText(preview.bytes, estimated)}
									{:else if pixels}
										{pixels.width.toLocaleString()} × {pixels.height.toLocaleString()} pixels
									{/if}
								</p>
								{#if estimated && preview}
									<p class="text-xs text-fg-muted">Previewed at {preview.dpi} dpi, as this page is large.</p>
								{/if}
							</div>
						</div>
						<div class="mt-5 flex justify-end gap-2">
							<button type="button" class="button" onclick={close}>Cancel</button>
							<button type="submit" class="button button-primary" disabled={!canExport} data-export-run>Export…</button>
						</div>
					</form>
				{/if}
			{/if}
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
