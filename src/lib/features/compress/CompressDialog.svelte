<script lang="ts">
	// The Compress dialog (section 6.11, ADR 0027), in three steps in one window: the
	// settings (with what the file's size is made of), the progress while Rust compresses
	// into a file of its own, and the result, with a before-and-after preview, to save over
	// the file, as a copy, or not at all. Closing it at any step forgets the file and its
	// result.
	import { ChevronDown, ChevronLeft, ChevronRight, ChevronUp } from '@lucide/svelte';
	import { Dialog } from 'bits-ui';
	import { untrack } from 'svelte';
	import { innerHeight } from 'svelte/reactivity/window';

	import Dropdown from '#lib/components/Dropdown.svelte';
	import { held } from '#lib/components/held.svelte.ts';
	import ProgressBar from '#lib/components/ProgressBar.svelte';
	import Segmented from '#lib/components/Segmented.svelte';
	import {
		compressCancel,
		compressPreviewUrl,
		compressRun,
		compressStop,
		toAppError,
		type CompressOutcome,
		type CompressPreset,
		type CompressProgress,
		type CompressSettings
	} from '#lib/ipc/index.ts';
	import { pdfaName } from '#lib/format.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	import { saveCompressed } from './actions.ts';
	import {
		COLOR_MODES,
		PRESETS,
		SIGNED_WARNING,
		details,
		formatSize,
		headline,
		notSmallerText,
		pdfaWarning,
		presetSettings,
		removalRows,
		renamed,
		settingsValid,
		simpleMono,
		simplified,
		sizeParts,
		stageText,
		stageValue
	} from './compress.ts';
	import DownsampleFields from './DownsampleFields.svelte';
	import ImageGroup from './ImageGroup.svelte';

	/** The last request, which the dialog goes on showing while it closes. */
	const last = held(() => app.compressRequest);
	const request = $derived(last.current);
	const asking = $derived(app.compressRequest !== null);

	type Step = 'settings' | 'running' | 'result';
	let step = $state<Step>('settings');
	let settings = $state<CompressSettings | null>(null);
	let progress = $state<CompressProgress | null>(null);
	let outcome = $state<CompressOutcome | null>(null);
	let saving = $state(false);
	/** The preview's page (0-based) and zoom. */
	let page = $state(0);
	let zoom = $state('1');

	// A new request: the settings used last, or Balanced.
	$effect(() => {
		const current = app.compressRequest;
		if (!current) return;
		untrack(() => {
			settings = current.remembered ?? current.presets.balanced;
			step = 'settings';
			progress = null;
			outcome = null;
			saving = false;
		});
	});

	const parts = $derived(request ? sizeParts(request.analysis) : []);
	const removals = $derived(request ? removalRows(request.analysis) : []);
	const valid = $derived(settings !== null && settingsValid(settings));
	const smaller = $derived(outcome !== null && outcome.after < outcome.before);
	const presetDetail = $derived(PRESETS.find((p) => p.value === settings?.preset)?.detail ?? '');

	/** Part `i`'s colour in the size bar: the accent, lighter part by part; the rest grey. */
	function partColor(key: string, i: number): string {
		if (key === 'other') return 'var(--lectrix-line)';
		return `color-mix(in srgb, var(--lectrix-accent) ${Math.max(25, 100 - i * 18)}%, transparent)`;
	}

	/** Changes the settings: renamed after the preset they match, and in the simple form
	 * made as it makes them. */
	function change(next: CompressSettings) {
		if (!request) return;
		settings = renamed(next.separate ? next : simplified(next), request.presets);
	}

	function choosePreset(preset: CompressPreset) {
		if (!request || !settings) return;
		if (preset === 'custom') {
			settings = { ...settings, preset };
			return;
		}
		settings = { ...presetSettings(request.presets, preset), separate: settings.separate };
	}

	function close() {
		const current = app.compressRequest;
		if (!current) return;
		app.compressRequest = null;
		void compressCancel(current.token);
	}

	async function run() {
		if (!request || !settings || !valid) return;
		const token = request.token;
		step = 'running';
		progress = null;
		try {
			outcome = await compressRun(token, settings, (p) => (progress = p));
			page = Math.min(outcome.previewPage, Math.max(0, outcome.pages - 1));
			base = null;
			step = 'result';
		} catch (e) {
			const error = toAppError(e);
			step = 'settings';
			if (error.code !== 'cancelled') app.showError(error);
		}
	}

	function stop() {
		if (request) void compressStop(request.token);
	}

	async function save(copy: boolean) {
		if (!request || saving) return;
		saving = true;
		try {
			if (await saveCompressed(request.token, copy)) {
				// Saved: Rust has forgotten the file and its result.
				app.compressRequest = null;
			}
		} catch (e) {
			app.showError(toAppError(e));
		} finally {
			saving = false;
		}
	}

	// ----- the preview -----

	let before = $state<HTMLElement | null>(null);
	let after = $state<HTMLElement | null>(null);
	/** The pane being scrolled by the user, which the other follows. */
	let leading: HTMLElement | null = null;

	function follow(from: HTMLElement | null, to: HTMLElement | null) {
		if (!from || !to || (leading && leading !== from)) return;
		leading = from;
		to.scrollLeft = from.scrollLeft;
		to.scrollTop = from.scrollTop;
		requestAnimationFrame(() => (leading = null));
	}

	const scale = $derived(Number(zoom) * (typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1));
	const ratio = $derived(typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1);
	/** The page's size at 100% in CSS pixels, as first loaded (both panes show the same page);
	 * kept while the next page loads, and not measured again at another zoom, whose image
	 * rounds to a slightly different size. */
	let base = $state<{ page: number; width: number; height: number } | null>(null);

	/** The width of a scroll bar, measured once. */
	let scrollbar = 0;
	function measureScrollbar(): number {
		const probe = document.createElement('div');
		probe.style.cssText = 'position:absolute;top:-200px;width:100px;height:100px;overflow:scroll';
		document.body.append(probe);
		const width = probe.offsetWidth - probe.clientWidth;
		probe.remove();
		return width;
	}

	let row = $state(0);
	/** The panes' box: the page at 100% with the scroll bars it needs there (and the 1 px
	 * border), up to half the row and a height the window leaves room for. The same at every
	 * zoom, so zooming changes only what the panes show. */
	const box = $derived.by(() => {
		const maxWidth = Math.floor(Math.max(0, (row - 12) / 2));
		const maxHeight = Math.min(720, Math.max(300, (innerHeight.current ?? 0) - 340));
		if (!base) return { width: maxWidth, height: maxHeight };
		const width = Math.ceil(base.width) + 2;
		const height = Math.ceil(base.height) + 2;
		let vertical = height > maxHeight;
		const horizontal = width + (vertical ? scrollbar : 0) > maxWidth;
		vertical ||= height + (horizontal ? scrollbar : 0) > maxHeight;
		return {
			width: Math.min(maxWidth, width + (vertical ? scrollbar : 0)),
			height: Math.min(maxHeight, height + (horizontal ? scrollbar : 0))
		};
	});
</script>

{#snippet pane(which: 'before' | 'after')}
	{#if request}
		<div class="flex min-w-0 flex-col gap-1">
			<span class="text-xs text-fg-muted">
				{which === 'before' ? 'Before' : 'After'} · {formatSize(which === 'before' ? (outcome?.before ?? 0) : (outcome?.after ?? 0))}
			</span>
			<!-- A scrolled region takes the focus, so the arrow keys scroll it (both panes). -->
			<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
			<div
				class="overflow-auto rounded-[6px] border border-line bg-canvas"
				style:width="{box.width}px"
				style:height="{box.height}px"
				bind:this={
					() => (which === 'before' ? before : after),
					(el) => {
						if (which === 'before') before = el;
						else after = el;
					}
				}
				onscroll={() => (which === 'before' ? follow(before, after) : follow(after, before))}
				tabindex="0"
				role="region"
				aria-label={which === 'before' ? 'The page as it is' : 'The page compressed'}
			>
				<img
					src={compressPreviewUrl(request.token, which === 'after', page, scale)}
					alt=""
					class="block max-w-none"
					style:width={base ? `${base.width * Number(zoom)}px` : undefined}
					style:height={base ? `${base.height * Number(zoom)}px` : undefined}
					onload={(e) => {
						const image = e.currentTarget as HTMLImageElement;
						scrollbar ||= measureScrollbar();
						if (base?.page === page) return;
						const at = ratio * Number(zoom);
						base = { page, width: image.naturalWidth / at, height: image.naturalHeight / at };
					}}
				/>
			</div>
		</div>
	{/if}
{/snippet}

<Dialog.Root
	open={asking}
	onOpenChange={(open) => {
		if (open) return;
		// Esc while compressing stops it; otherwise the dialog closes, writing nothing.
		if (step === 'running') stop();
		else if (!saving) close();
	}}
>
	<Dialog.Portal>
		<Dialog.Overlay class="dialog-overlay" />
		<Dialog.Content
			class="dialog-content flex max-h-[calc(100vh-48px)] flex-col {step === 'result' && smaller ? 'w-[1000px]' : 'w-[600px]'}"
			data-compress-dialog
			data-step={step}
			interactOutsideBehavior="ignore"
		>
			{#if request && settings}
				<Dialog.Title class="text-base font-semibold">Compress {request.name}</Dialog.Title>
				<Dialog.Description class="mt-1 text-sm text-fg-muted">
					{formatSize(request.analysis.fileSize)} · {request.analysis.pages === 1
						? '1 page'
						: `${request.analysis.pages.toLocaleString()} pages`}
				</Dialog.Description>

				{#if step === 'settings'}
					<div class="-mx-6 mt-4 flex min-h-0 flex-col gap-5 overflow-y-auto px-6" data-compress-settings>
						{#if parts.length > 0}
							<div class="flex flex-col gap-2" data-compress-sizes>
								<div class="flex h-2 overflow-hidden rounded-full bg-bg" aria-hidden="true">
									{#each parts as part, i (part.key)}
										{#if part.bytes / request.analysis.fileSize >= 0.01}
											<div
												style:width="{(part.bytes / request.analysis.fileSize) * 100}%"
												style:background={partColor(part.key, i)}
											></div>
										{/if}
									{/each}
								</div>
								<ul class="flex flex-wrap gap-x-4 gap-y-1 text-xs text-fg-muted" aria-label="What the file’s size is made of">
									{#each parts as part, i (part.key)}
										<li class="flex items-center gap-1.5">
											<span class="h-2 w-2 rounded-full" style:background={partColor(part.key, i)} aria-hidden="true"></span>
											{part.label}
											<span class="text-fg tabular-nums">{formatSize(part.bytes)}</span>
										</li>
									{/each}
								</ul>
							</div>
						{/if}

						<div class="flex flex-col gap-1">
							<div class="flex items-center gap-3">
								<span class="w-[96px] font-medium" aria-hidden="true">Preset</span>
								<Dropdown
									class="h-8 w-[200px]"
									value={settings.preset}
									label="Preset"
									options={PRESETS}
									onchange={choosePreset}
								/>
							</div>
							<p class="ml-[108px] text-sm text-fg-muted">{presetDetail}</p>
						</div>

						<fieldset class="flex flex-col gap-3">
							<legend class="mb-2 font-medium">Images</legend>
							{#if settings.separate}
								<div class="flex flex-col gap-2">
									<h3 class="text-sm font-medium">Colour images</h3>
									<ImageGroup kind="colour" value={settings.color} onchange={(color) => settings && change({ ...settings, color })} />
								</div>
								<div class="flex flex-col gap-2">
									<h3 class="text-sm font-medium">Greyscale images</h3>
									<ImageGroup kind="greyscale" value={settings.gray} onchange={(gray) => settings && change({ ...settings, gray })} />
								</div>
								<div class="flex flex-col gap-2">
									<h3 class="text-sm font-medium">Black-and-white images</h3>
									<DownsampleFields
										label="Downsample black-and-white images"
										value={settings.mono}
										onchange={(mono) => settings && change({ ...settings, mono })}
									/>
									<p class="ml-6 text-sm text-fg-muted">Always lossless: CCITT Group 4 or ZIP, whichever is smaller.</p>
								</div>
							{:else}
								<ImageGroup value={settings.color} onchange={(color) => settings && change({ ...settings, color })} />
								<p class="ml-6 text-sm text-fg-muted">
									{#if settings.color.downsample.enabled}
										Black-and-white images: {simpleMono(settings.color.downsample).toDpi} dpi, always lossless.
									{:else}
										Black-and-white images: kept at their resolution, always lossless.
									{/if}
								</p>
							{/if}
							<div class="ml-6 flex items-center gap-2">
								<span class="w-[96px] text-sm" aria-hidden="true">Colour</span>
								<Dropdown
									class="h-7 w-[180px]"
									value={settings.colorMode}
									label="Colour of images"
									options={COLOR_MODES}
									onchange={(colorMode) => settings && change({ ...settings, colorMode })}
								/>
							</div>
							<button
								type="button"
								class="flex items-center gap-1 self-start text-sm text-accent"
								aria-expanded={settings.separate}
								onclick={() => settings && change({ ...settings, separate: !settings.separate })}
							>
								{#if settings.separate}<ChevronUp size={16} aria-hidden="true" />{:else}<ChevronDown size={16} aria-hidden="true" />{/if}
								Separate settings for colour, greyscale and black-and-white images
							</button>
						</fieldset>

						<fieldset class="flex flex-col gap-2">
							<legend class="mb-2 font-medium">Fonts and structure</legend>
							<label class="flex items-start gap-2">
								<input
									type="checkbox"
									class="mt-1"
									checked={settings.subsetFonts}
									onchange={(e) => settings && change({ ...settings, subsetFonts: e.currentTarget.checked })}
								/>
								<span>
									Subset embedded fonts
									<span class="block text-sm text-fg-muted">
										Keeps only the characters the document uses. Fonts that form fields type with stay whole.
									</span>
								</span>
							</label>
							<label class="flex items-start gap-2">
								<input
									type="checkbox"
									class="mt-1"
									checked={settings.objectStreams}
									onchange={(e) => settings && change({ ...settings, objectStreams: e.currentTarget.checked })}
								/>
								<span>
									Compress the file’s structure
									<span class="block text-sm text-fg-muted">
										Packs its objects together. Needs PDF 1.5, which every current reader opens.
									</span>
								</span>
							</label>
						</fieldset>

						{#if removals.length > 0}
							<fieldset class="flex flex-col gap-2" data-compress-removals>
								<legend class="mb-2 font-medium">Remove</legend>
								{#each removals as row (row.key)}
									<label class="flex items-start gap-2">
										<input
											type="checkbox"
											class="mt-1"
											checked={settings.remove[row.key]}
											onchange={(e) =>
												settings && change({ ...settings, remove: { ...settings.remove, [row.key]: e.currentTarget.checked } })}
										/>
										<span>
											{row.label}
											<span class="block text-sm text-fg-muted">{row.detail}</span>
										</span>
									</label>
								{/each}
							</fieldset>
						{/if}

						{#if request.signed || request.pdfa}
							<div class="flex flex-col gap-1 text-sm text-fg-muted">
								{#if request.signed}<p>{SIGNED_WARNING}</p>{/if}
								{#if request.pdfa}<p>{pdfaWarning(pdfaName(request.pdfa))}</p>{/if}
							</div>
						{/if}
					</div>
					<div class="mt-5 flex justify-end gap-2">
						<button type="button" class="button" onclick={close}>Cancel</button>
						<button type="button" class="button button-primary" disabled={!valid} onclick={run} data-compress-run>Compress</button>
					</div>
				{:else if step === 'running'}
					<div class="mt-6 flex flex-col gap-2" data-compress-progress>
						<ProgressBar value={progress ? stageValue(progress) : null} label="Compressing" />
						<p class="text-sm text-fg-muted" aria-live="polite">{progress ? stageText(progress) : 'Opening the file…'}</p>
					</div>
					<div class="mt-6 flex justify-end">
						<button type="button" class="button" onclick={stop} data-compress-stop>Stop</button>
					</div>
				{:else if outcome}
					<div class="-mx-6 mt-4 flex min-h-0 flex-col gap-4 overflow-y-auto px-6" data-compress-result>
						{#if smaller}
							<div class="flex flex-col gap-1">
								<p class="text-lg font-semibold" data-compress-headline>{headline(outcome)}</p>
								{#each details(outcome) as line (line)}
									<p class="text-sm text-fg-muted">{line}</p>
								{/each}
							</div>
							<div class="flex flex-col gap-2">
								<div class="flex items-center gap-2">
									<button
										type="button"
										class="icon-button"
										data-tip="Previous page"
										aria-label="Previous page"
										disabled={page <= 0}
										onclick={() => (page = Math.max(0, page - 1))}
									>
										<ChevronLeft size={16} aria-hidden="true" />
									</button>
									<span class="text-sm tabular-nums">Page {page + 1} of {outcome.pages}</span>
									<button
										type="button"
										class="icon-button"
										data-tip="Next page"
										aria-label="Next page"
										disabled={page >= outcome.pages - 1}
										onclick={() => outcome && (page = Math.min(outcome.pages - 1, page + 1))}
									>
										<ChevronRight size={16} aria-hidden="true" />
									</button>
									<div class="ml-auto">
										<Segmented
											value={zoom}
											options={[
												{ value: '1', label: '100%' },
												{ value: '2', label: '200%' },
												{ value: '4', label: '400%' }
											]}
											onchange={(z) => (zoom = z)}
										/>
									</div>
								</div>
								<div class="flex justify-center gap-3" bind:clientWidth={row}>
									{@render pane('before')}
									{@render pane('after')}
								</div>
							</div>
						{:else}
							<p data-compress-not-smaller>{notSmallerText(request.name, outcome)}</p>
						{/if}
					</div>
					<div class="mt-5 flex justify-end gap-2">
						{#if smaller}
							<button type="button" class="button" onclick={close} disabled={saving}>Cancel</button>
							<button type="button" class="button" onclick={() => (step = 'settings')} disabled={saving}>Back to settings</button>
							<button type="button" class="button" onclick={() => save(true)} disabled={saving} data-compress-copy>Save as a copy</button>
							<button type="button" class="button button-primary" onclick={() => save(false)} disabled={saving} data-compress-save>Save</button>
						{:else}
							<button type="button" class="button" onclick={() => (step = 'settings')}>Back to settings</button>
							<button type="button" class="button button-primary" onclick={close}>Close</button>
						{/if}
					</div>
				{/if}
			{/if}
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
