<script lang="ts">
	// The Images in the document dialog (section 6.14, ADR 0035): the pages, whether small
	// images are left out and how files are named, with the images that will be exported
	// beside them, each once. Export… asks where in the Save dialog (Rust), then, if some
	// files are there already, whether to replace them; the export shows progress and Stop.
	import { Dialog } from 'bits-ui';
	import { untrack } from 'svelte';

	import Dropdown from '#lib/components/Dropdown.svelte';
	import { held } from '#lib/components/held.svelte.ts';
	import ProgressBar from '#lib/components/ProgressBar.svelte';
	import Segmented from '#lib/components/Segmented.svelte';
	import { pageName } from '#lib/features/viewer/pagebox.ts';
	import {
		embeddedThumbnailUrl,
		exportShowInFolder,
		imagesFind,
		imagesForget,
		imagesPick,
		imagesRun,
		imagesStart,
		imagesStop,
		toAppError,
		type AppError,
		type ExportProgress,
		type ExportTarget,
		type FileNaming,
		type ImagesFound,
		type ImagesSettings
	} from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	import { SHOW_IN_FOLDER, existingText, pagesText, pagesToExport, stemOf, type PagesChoice } from './export.ts';
	import {
		DEFAULT_IMAGES_PREFERENCES,
		MIN_SIDES,
		exampleImageName,
		imageTip,
		imagesDoneMessage,
		imagesText,
		leftOutText,
		noneText,
		summaryText
	} from './images.ts';

	/** The most thumbnails shown; the rest are counted. */
	const MOST_SHOWN = 300;

	const request = $derived(app.imagesRequest);
	const last = held(() => (request ? (app.tabs.find((t) => t.id === request.tabId) ?? null) : null));
	/** The document whose images are exported; null once the dialog closes. */
	const tab = $derived(request ? (app.tabs.find((t) => t.id === request.tabId) ?? null) : null);
	const shown = $derived(last.current);
	/** The pages selected where the dialog was opened (Organize pages). */
	const selected = $derived(request?.selected ?? null);

	let step = $state<'settings' | 'confirm' | 'exporting'>('settings');
	let problem = $state<AppError | null>(null);

	let choice = $state<PagesChoice>('all');
	let rangeText = $state('');
	let current = $state(0);
	let skipSmall = $state(true);
	let minSide = $state(32);
	let naming = $state<FileNaming>('numbers');
	let ready = $state(false);

	let found = $state<ImagesFound | null>(null);
	let finding = $state(false);
	let findProgress = $state<ExportProgress | null>(null);
	let findProblem = $state<AppError | null>(null);
	let target = $state<ExportTarget | null>(null);
	let progress = $state<ExportProgress | null>(null);
	/** Each opening of the dialog, and each search asked for: what an earlier one asked is
	 * then ignored. */
	let opening = 0;
	let asking = 0;

	const labels = $derived(shown?.labels ?? null);
	const count = $derived(shown?.pages.length ?? 0);
	const pages = $derived(
		shown ? pagesToExport({ choice, rangeText, current, selected: selected ?? [] }, labels, count) : null
	);
	const settings = $derived<ImagesSettings | null>(pages ? { pages, skipSmall, minSide, naming } : null);
	const images = $derived(found?.images ?? []);
	const canExport = $derived(ready && settings !== null && !finding && images.length > 0 && step === 'settings');

	function close() {
		if (tab) void imagesForget(tab.id);
		app.imagesRequest = null;
	}

	// A new request: start afresh, with the choices used last.
	$effect(() => {
		const r = request;
		if (!r) return;
		untrack(() => void open());
	});

	async function open() {
		const mine = ++opening;
		const r = request;
		const doc = tab;
		if (!r || !doc) return;
		step = 'settings';
		problem = null;
		found = null;
		findProblem = null;
		target = null;
		progress = null;
		ready = false;
		current = doc.currentPage;
		choice = r.selected ? 'selected' : 'all';
		rangeText = '';
		try {
			const prefs = (await imagesStart()) ?? DEFAULT_IMAGES_PREFERENCES;
			if (mine !== opening) return;
			skipSmall = prefs.skipSmall;
			minSide = MIN_SIDES.find((m) => m === prefs.minSide) ?? DEFAULT_IMAGES_PREFERENCES.minSide;
			naming = prefs.naming;
		} catch (e) {
			if (mine === opening) problem = toAppError(e);
		}
		if (mine === opening) ready = true;
	}

	// The images, found again a moment after the pages or what is left out change (Rust
	// reads the pages again only when they change).
	$effect(() => {
		const doc = tab;
		const wanted = settings;
		if (!doc || !wanted || !ready || step !== 'settings') return;
		const mine = ++asking;
		finding = true;
		findProgress = null;
		const timer = setTimeout(async () => {
			try {
				const result = await imagesFind(doc.id, $state.snapshot(wanted) as ImagesSettings, (p) => {
					if (mine === asking) findProgress = p;
				});
				if (mine !== asking) return;
				found = result;
				findProblem = null;
			} catch (e) {
				const error = toAppError(e);
				if (mine !== asking || error.code === 'cancelled') return;
				found = null;
				findProblem = error;
			} finally {
				if (mine === asking) finding = false;
			}
		}, 150);
		return () => clearTimeout(timer);
	});

	async function pick(event?: SubmitEvent) {
		event?.preventDefault();
		const doc = tab;
		const wanted = settings;
		if (!doc || !wanted || !canExport) return;
		problem = null;
		try {
			const planned = await imagesPick(doc.id, $state.snapshot(wanted) as ImagesSettings);
			if (!planned) return;
			target = planned;
			if (planned.existingCount > 0) step = 'confirm';
			else await run(false);
		} catch (e) {
			problem = toAppError(e);
		}
	}

	async function run(skipExisting: boolean) {
		const doc = tab;
		if (!doc) return;
		const name = doc.name;
		step = 'exporting';
		progress = null;
		try {
			const done = await imagesRun(doc.id, skipExisting, (p) => (progress = p));
			app.imagesRequest = null;
			app.notify({
				kind: 'info',
				message: imagesDoneMessage(name, done),
				action: done.files > 0 ? { label: SHOW_IN_FOLDER, run: () => void showFolder(done.token) } : undefined
			});
		} catch (e) {
			const error = toAppError(e);
			app.imagesRequest = null;
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
		if (tab) void imagesStop(tab.id);
	}

	// ----- what the dialog shows -----

	const sizeOptions = MIN_SIDES.map((m) => ({ value: String(m), label: `${m} pixels` }));
	const stem = $derived(stemOf(shown?.name ?? 'Document'));
	const most = $derived(images.reduce((m, i) => Math.max(m, i.index), 1));
	const example = $derived(images[0] ? exampleImageName(stem, images[0], count, most, naming, labels) : '');
	const minShown = $derived(skipSmall ? minSide : null);
	const thumbnails = $derived(images.slice(0, MOST_SHOWN));
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
			data-images-dialog
			data-step={step}
			interactOutsideBehavior="ignore"
		>
			{#if shown}
				<Dialog.Title class="truncate text-base font-semibold">Export the images in {shown.name}</Dialog.Title>
				<Dialog.Description class="mt-1 text-sm text-fg-muted">
					{pagesText(count)}{shown.state.dirty ? ', with its unsaved changes' : ''}
				</Dialog.Description>

				{#if step === 'exporting'}
					<div class="mt-6 flex flex-col gap-2" data-images-progress>
						<ProgressBar value={progress ? progress.done : null} max={progress?.total || 1} label="Exporting" />
						<p class="text-sm text-fg-muted" aria-live="polite">
							{progress && progress.total > 0
								? `Exporting image ${Math.min(progress.done + 1, progress.total).toLocaleString()} of ${progress.total.toLocaleString()}…`
								: 'Getting the images ready…'}
						</p>
					</div>
					<div class="mt-6 flex justify-end">
						<button type="button" class="button" onclick={stop} data-images-stop>Stop</button>
					</div>
				{:else if step === 'confirm' && target}
					<div class="mt-5 flex flex-col gap-2" data-images-confirm>
						<p>{existingText(target)}</p>
						<p class="text-sm text-fg-muted">
							Replace them with the images exported now, or keep them and export only the images whose files aren’t there.
						</p>
					</div>
					<div class="mt-6 flex justify-end gap-2">
						<button type="button" class="button" onclick={() => (step = 'settings')}>Back</button>
						<button type="button" class="button" onclick={() => void run(true)} data-images-skip>Keep them</button>
						<button type="button" class="button button-primary" onclick={() => void run(false)} data-images-replace>
							Replace
						</button>
					</div>
				{:else}
					<form class="mt-4 flex min-h-0 flex-col" onsubmit={pick}>
						<div class="-mx-6 flex min-h-0 gap-6 overflow-y-auto overflow-x-hidden px-6">
							<div class="flex w-[380px] shrink-0 flex-col gap-4" data-images-settings>
								<fieldset class="flex flex-col gap-2">
									<legend class="mb-1 font-medium">Pages</legend>
									<label class="flex items-center gap-2">
										<input type="radio" bind:group={choice} value="all" />
										All {pagesText(count)}
									</label>
									<label class="flex items-center gap-2">
										<input type="radio" bind:group={choice} value="current" />
										The current page ({pageName(current, labels)})
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
											aria-label="Pages whose images to export"
											aria-invalid={choice === 'range' && rangeText.trim() !== '' && pages === null}
											data-images-range
										/>
									</label>
									{#if choice === 'range' && rangeText.trim() !== '' && pages === null}
										<p class="ml-6 text-sm text-danger" role="alert">
											Type pages as {labels ? 'they are numbered, as i-iv, 1-5, 8' : '1-5, 8, 11-13'}, up to {count}.
										</p>
									{/if}
								</fieldset>

								<fieldset class="flex flex-col gap-2">
									<legend class="mb-1 font-medium">Images</legend>
									<div class="flex items-center gap-2">
										<label class="flex items-center gap-2">
											<input type="checkbox" bind:checked={skipSmall} data-images-skip-small />
											Leave out images smaller than
										</label>
										<Dropdown
											class="h-7 w-[110px]"
											value={String(minSide)}
											label="Smallest image exported"
											options={sizeOptions}
											disabled={!skipSmall}
											onchange={(m) => (minSide = Number(m))}
										/>
									</div>
									<p class="text-sm text-fg-muted">
										JPEG and JPEG 2000 images are saved as the document keeps them; the others as PNG, with their transparency.
									</p>
								</fieldset>

								{#if images.length > 1}
									<fieldset class="flex flex-col gap-2">
										<legend class="mb-1 font-medium">Files</legend>
										{#if labels}
											<div class="flex items-center gap-2">
												<span id="images-naming" class="w-[88px] shrink-0 text-sm">Name by</span>
												<Segmented
													value={naming}
													labelledby="images-naming"
													options={[
														{ value: 'numbers', label: 'Page numbers' },
														{ value: 'labels', label: 'Page labels' }
													]}
													onchange={(n) => (naming = n as FileNaming)}
												/>
											</div>
										{/if}
										<p class="text-sm text-fg-muted" data-images-naming-example>
											A file an image, named after the name you give, its page and its place on the page: {example}, and so on.
										</p>
									</fieldset>
								{/if}

								{#if problem}
									<p class="text-sm text-danger" role="alert">
										{problem.message}{problem.suggestion ? ` ${problem.suggestion}` : ''}
									</p>
								{/if}
							</div>

							<div class="flex min-w-0 flex-1 flex-col gap-2" data-images-found>
								<div
									class="relative overflow-y-auto overflow-x-hidden rounded-[6px] bg-canvas p-3"
									style:width="364px"
									style:height="424px"
									data-images-grid
								>
									{#if found && images.length > 0}
										<ul class="grid grid-cols-3 gap-2" aria-label="Images to export">
											{#each thumbnails as image (image.key)}
												{@const tip = imageTip(image, pageName(image.page, labels))}
												<li
													class="flex h-[104px] items-center justify-center rounded-[4px] bg-surface p-1"
													data-tip={tip}
													data-images-thumbnail
												>
													<img
														src={tab ? embeddedThumbnailUrl(tab.id, image.num, image.key) : ''}
														alt={tip}
														loading="lazy"
														decoding="async"
														class="max-h-full max-w-full bg-white object-contain shadow-[0_1px_4px_var(--color-page-shadow)]"
													/>
												</li>
											{/each}
										</ul>
										{#if images.length > thumbnails.length}
											<p class="mt-3 text-center text-sm text-fg-muted">
												And {imagesText(images.length - thumbnails.length)} more.
											</p>
										{/if}
									{:else if findProblem}
										<p class="flex h-full items-center justify-center px-6 text-center text-sm text-fg-muted">
											{findProblem.message}
										</p>
									{:else if found && !finding}
										<p class="flex h-full items-center justify-center px-6 text-center text-sm text-fg-muted" data-images-none>
											{noneText(found, minShown, pages?.length ?? 0)}
										</p>
									{:else}
										<p class="flex h-full items-center justify-center text-sm text-fg-muted" aria-live="polite">
											{findProgress && findProgress.total > 8
												? `Looking for images… page ${findProgress.done.toLocaleString()} of ${findProgress.total.toLocaleString()}`
												: 'Looking for images…'}
										</p>
									{/if}
									{#if finding && found}
										<span
											class="absolute right-2 top-2 rounded-[4px] bg-surface px-2 py-0.5 text-xs text-fg-muted shadow-sm"
											aria-live="polite">Updating…</span
										>
									{/if}
								</div>
								{#if found && images.length > 0}
									<p class="text-sm" data-images-summary>{summaryText(found)}</p>
								{/if}
								{#if found && leftOutText(found, minShown)}
									<p class="text-xs text-fg-muted" data-images-left-out>{leftOutText(found, minShown)}</p>
								{/if}
							</div>
						</div>
						<div class="mt-5 flex justify-end gap-2">
							<button type="button" class="button" onclick={close}>Cancel</button>
							<button type="submit" class="button button-primary" disabled={!canExport} data-images-run>Export…</button>
						</div>
					</form>
				{/if}
			{/if}
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
