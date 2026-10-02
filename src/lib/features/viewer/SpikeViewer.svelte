<script lang="ts">
	// Phase 0 spike: proves that pages render in the window through the folio:// protocol
	// and shows the measured timings. The real viewer (continuous scroll, tabs, thumbnails)
	// is Phase 1.
	import { ChevronLeft, ChevronRight, FolderOpen } from '@lucide/svelte';
	import { onMount } from 'svelte';

	import { APP_NAME } from '#lib/config.ts';
	import {
		appReady,
		closeDocument,
		isAppError,
		logMetric,
		openStartupDocument,
		openWithDialog,
		pageUrl,
		parseTiming,
		type AppError,
		type DocumentInfo,
		type PageTiming
	} from '#lib/ipc/index.ts';

	const ZOOMS = [0.5, 0.75, 1, 1.25, 1.5, 2];
	const CSS_PX_PER_PT = 96 / 72;

	let doc = $state<DocumentInfo | null>(null);
	let pageIndex = $state(0);
	let zoom = $state(1);
	let imageUrl = $state<string | null>(null);
	let error = $state<AppError | null>(null);
	let loading = $state(false);

	let startupMs = $state<number | null>(null);
	let firstPageMs = $state<number | null>(null);
	let lastTiming = $state<PageTiming | null>(null);
	let lastFetchMs = $state<number | null>(null);

	// Set when a document opens; cleared once its first page is on screen.
	let openStartedAt: number | null = null;
	let requestSeq = 0;

	const page = $derived(doc?.pages[pageIndex] ?? null);
	const cssWidth = $derived(page ? page.width * zoom * CSS_PX_PER_PT : 0);
	const cssHeight = $derived(page ? page.height * zoom * CSS_PX_PER_PT : 0);

	onMount(() => {
		void startup();
		return () => {
			if (imageUrl) URL.revokeObjectURL(imageUrl);
		};
	});

	async function startup() {
		try {
			startupMs = (await appReady()).mainToReadyMs;
			void logMetric('main_to_ready_ms', startupMs);
			openStartedAt = performance.now();
			const info = await openStartupDocument();
			if (info) show(info);
			else openStartedAt = null;
		} catch (e) {
			fail(e);
		}
	}

	async function open() {
		try {
			const t0 = performance.now();
			const info = await openWithDialog();
			if (!info) return;
			// Exclude the time the user spent in the dialog: count from its result.
			openStartedAt = performance.now() - info.openMs;
			void t0;
			show(info);
		} catch (e) {
			fail(e);
		}
	}

	function show(info: DocumentInfo) {
		if (doc) void closeDocument(doc.id);
		error = null;
		firstPageMs = null;
		doc = info;
		pageIndex = 0;
	}

	function fail(e: unknown) {
		error = isAppError(e)
			? e
			: { message: 'Something went wrong.', suggestion: 'Try again.' };
	}

	// Fetch the page image whenever the document, page or zoom changes.
	$effect(() => {
		if (!doc || !page) return;
		const scale = zoom * CSS_PX_PER_PT * (window.devicePixelRatio || 1);
		void load(pageUrl(doc.id, pageIndex, scale, 0));
	});

	async function load(url: string) {
		const seq = ++requestSeq;
		loading = true;
		const t0 = performance.now();
		try {
			const response = await fetch(url);
			if (!response.ok) throw new Error(await response.text());
			const blob = await response.blob();
			if (seq !== requestSeq) return;
			lastFetchMs = performance.now() - t0;
			lastTiming = parseTiming(response.headers.get('X-Folio-Timing'));
			const next = URL.createObjectURL(blob);
			if (imageUrl) URL.revokeObjectURL(imageUrl);
			imageUrl = next;
		} catch {
			if (seq === requestSeq)
				error = { message: 'This page could not be displayed.', suggestion: 'Try another page or zoom level.' };
		} finally {
			if (seq === requestSeq) loading = false;
		}
	}

	function imageShown() {
		if (openStartedAt !== null) {
			firstPageMs = performance.now() - openStartedAt;
			openStartedAt = null;
			void logMetric('first_page_visible_ms', firstPageMs);
		}
	}

	function go(delta: number) {
		if (!doc) return;
		pageIndex = Math.min(doc.pageCount - 1, Math.max(0, pageIndex + delta));
	}

	function onKey(event: KeyboardEvent) {
		if (event.ctrlKey && event.key.toLowerCase() === 'o') {
			event.preventDefault();
			void open();
		} else if (event.key === 'PageDown') {
			go(1);
		} else if (event.key === 'PageUp') {
			go(-1);
		}
	}

	const ms = (v: number | null) => (v === null ? '–' : `${v.toFixed(0)} ms`);
	const encodeShare = $derived(
		lastTiming
			? (100 * lastTiming.encodeMs) / Math.max(lastTiming.displayListMs + lastTiming.rasterMs, 0.001)
			: null
	);
</script>

<svelte:window onkeydown={onKey} />

<div class="flex h-full flex-col bg-bg text-fg">
	<header class="flex h-12 shrink-0 items-center gap-2 border-b border-line bg-surface px-3">
		<span class="mr-2 font-semibold">{APP_NAME}</span>
		<button
			type="button"
			class="inline-flex h-8 items-center gap-2 rounded-control px-3 hover:bg-surface-raised"
			onclick={open}
		>
			<FolderOpen size={16} aria-hidden="true" />
			Open…
		</button>
		{#if doc}
			<span class="truncate text-fg-muted" title={doc.name}>{doc.name}</span>
			<div class="ml-auto flex items-center gap-1">
				<button
					type="button"
					class="inline-flex size-8 items-center justify-center rounded-control hover:bg-surface-raised disabled:opacity-40"
					aria-label="Previous page"
					disabled={pageIndex === 0}
					onclick={() => go(-1)}
				>
					<ChevronLeft size={16} aria-hidden="true" />
				</button>
				<span class="min-w-24 text-center tabular-nums" aria-live="polite">
					{pageIndex + 1} of {doc.pageCount}
				</span>
				<button
					type="button"
					class="inline-flex size-8 items-center justify-center rounded-control hover:bg-surface-raised disabled:opacity-40"
					aria-label="Next page"
					disabled={pageIndex >= doc.pageCount - 1}
					onclick={() => go(1)}
				>
					<ChevronRight size={16} aria-hidden="true" />
				</button>
				<label class="ml-2 flex items-center gap-2">
					<span class="sr-only">Zoom</span>
					<select
						class="h-8 rounded-control border border-line bg-surface-raised px-2"
						bind:value={zoom}
					>
						{#each ZOOMS as z (z)}
							<option value={z}>{Math.round(z * 100)}%</option>
						{/each}
					</select>
				</label>
			</div>
		{/if}
	</header>

	{#if error}
		<div role="alert" class="border-b border-line bg-surface-raised px-4 py-2">
			<strong>{error.message}</strong>
			{#if error.suggestion}<span class="text-fg-muted"> {error.suggestion}</span>{/if}
		</div>
	{/if}

	<main class="flex flex-1 justify-center overflow-auto bg-canvas p-6">
		{#if doc && page}
			<div
				class="relative shrink-0 bg-white shadow-[0_1px_4px_var(--color-page-shadow)]"
				style:width="{cssWidth}px"
				style:height="{cssHeight}px"
				aria-busy={loading}
			>
				{#if imageUrl}
					<img
						src={imageUrl}
						alt="Page {pageIndex + 1}"
						class="block size-full"
						onload={imageShown}
						draggable="false"
					/>
				{/if}
			</div>
		{:else}
			<div class="m-auto text-center text-fg-muted">
				<p>Open a PDF to see it here.</p>
				<p class="mt-1 text-sm">Ctrl+O</p>
			</div>
		{/if}
	</main>

	<footer
		class="flex h-8 shrink-0 items-center gap-4 border-t border-line bg-surface px-3 text-sm text-fg-muted tabular-nums"
	>
		<span>Startup {ms(startupMs)}</span>
		{#if doc}
			<span>Open {ms(doc.openMs)}</span>
			<span>First page visible {ms(firstPageMs)}</span>
			{#if lastTiming}
				<span>
					Display list {lastTiming.displayListMs.toFixed(1)} · raster {lastTiming.rasterMs.toFixed(1)} ·
					encode {lastTiming.encodeMs.toFixed(1)} ms ({encodeShare?.toFixed(0)}% of render)
				</span>
			{/if}
			<span>Fetch {ms(lastFetchMs)}</span>
		{/if}
	</footer>
</div>
