<script lang="ts">
	// Find in document (Ctrl+F): searches as you type, Enter / Shift+Enter (or F3) for the
	// next and previous match.
	import { ChevronDown, ChevronUp, X } from '@lucide/svelte';
	import { onMount, untrack } from 'svelte';

	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	let { tab }: { tab: DocTab } = $props();

	let input: HTMLInputElement | undefined = $state();
	// Starts from the last query when the bar reopens; then the input owns the text.
	let value = $state(untrack(() => tab.search.query));
	let timer: ReturnType<typeof setTimeout> | undefined;

	export function focus() {
		input?.focus();
		input?.select();
	}

	onMount(() => {
		focus();
		return () => clearTimeout(timer);
	});

	function onInput() {
		clearTimeout(timer);
		timer = setTimeout(() => void tab.search.start(value, tab.currentPage), 250);
	}

	function onKeyDown(event: KeyboardEvent) {
		if (event.key === 'Enter') {
			event.preventDefault();
			if (value !== tab.search.query) {
				clearTimeout(timer);
				void tab.search.start(value, tab.currentPage);
			} else {
				tab.search.step(event.shiftKey ? -1 : 1);
			}
		} else if (event.key === 'Escape') {
			event.preventDefault();
			event.stopPropagation();
			close();
		}
	}

	function close() {
		tab.search.open = false;
		tab.search.clear();
		tab.search.query = '';
		tab.viewer?.focus();
	}

	const status = $derived.by(() => {
		const s = tab.search;
		if (!s.query) return '';
		if (s.hits.length === 0) return s.running ? 'Searching…' : 'No results';
		const position = s.current >= 0 ? `${s.current + 1} of ${s.hits.length}` : `${s.hits.length}`;
		return s.running ? `${position}…` : position;
	});
</script>

<div
	class="absolute right-6 z-10 flex items-center gap-1 rounded-panel border border-line bg-surface-raised p-1 shadow-[0_4px_12px_var(--color-page-shadow)]"
	style:top="{app.overlayTop}px"
	role="search"
>
	<input
		bind:this={input}
		bind:value
		oninput={onInput}
		onkeydown={onKeyDown}
		class="field h-8 w-56"
		placeholder="Find in document"
		aria-label="Find in document"
		data-search-input
	/>
	<span class="min-w-20 px-1 text-center text-sm text-fg-muted tabular-nums" aria-live="polite">
		{status}
	</span>
	<button
		type="button"
		class="icon-button"
		aria-label="Previous match (Shift+Enter)"
		title="Previous match (Shift+Enter)"
		disabled={tab.search.hits.length === 0}
		onclick={() => tab.search.step(-1)}
	>
		<ChevronUp size={16} aria-hidden="true" />
	</button>
	<button
		type="button"
		class="icon-button"
		aria-label="Next match (Enter)"
		title="Next match (Enter)"
		disabled={tab.search.hits.length === 0}
		onclick={() => tab.search.step(1)}
	>
		<ChevronDown size={16} aria-hidden="true" />
	</button>
	<button type="button" class="icon-button" aria-label="Close find (Esc)" title="Close (Esc)" onclick={close}>
		<X size={16} aria-hidden="true" />
	</button>
</div>
