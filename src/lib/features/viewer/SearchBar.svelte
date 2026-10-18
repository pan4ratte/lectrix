<script lang="ts">
	// Find in document (Ctrl+F): searches as you type, without moving the view; Enter /
	// Shift+Enter (or F3) go to the next and previous match. Under the field, the words of the document that contain what
	// is typed, with how many matches each has (section 6.1), as a combobox's list.
	import { ChevronDown, ChevronUp, X } from '@lucide/svelte';
	import { onMount, untrack } from 'svelte';
	import { cubicOut } from 'svelte/easing';
	import { fly } from 'svelte/transition';

	import { motionMs } from '#lib/components/panes.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { searchGlides } from './search.ts';
	import { splitMatch, type Suggestion } from './suggestions.ts';

	let { tab }: { tab: DocTab } = $props();

	const LIST_ID = `search-suggestions-${untrack(() => tab.id)}`;

	let input: HTMLInputElement | undefined = $state();
	let timer: ReturnType<typeof setTimeout> | undefined;
	let focused = $state(false);
	/** The list shows once something is typed, until a match is gone to or Esc. */
	let listOpen = $state(false);
	/** The suggestion picked with the arrow keys or under the pointer: -1 for none. */
	let active = $state(-1);

	const search = $derived(tab.search);
	// The suggestions belong to the query searched, which follows the field once typing
	// pauses: until then the list waits, rather than showing another query's words.
	const listShown = $derived(listOpen && focused && search.suggestions.length > 0 && search.query === search.input);

	export function focus() {
		input?.focus();
		input?.select();
	}

	onMount(() => {
		focus();
		return () => clearTimeout(timer);
	});

	function onInput() {
		listOpen = true;
		active = -1;
		clearTimeout(timer);
		timer = setTimeout(() => void search.start(search.input, tab.currentPage), 250);
	}

	function pick(s: Suggestion) {
		clearTimeout(timer);
		search.input = s.text;
		listOpen = false;
		active = -1;
		void search.start(s.text, tab.currentPage, { go: 'reveal', smooth: searchGlides() });
	}

	function onKeyDown(event: KeyboardEvent) {
		const n = search.suggestions.length;
		if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
			if (n === 0 || search.query !== search.input) return;
			event.preventDefault();
			if (!listShown) {
				listOpen = true;
				active = event.key === 'ArrowDown' ? 0 : n - 1;
			} else if (event.key === 'ArrowDown') active = (active + 1) % n;
			else active = active <= 0 ? n - 1 : active - 1;
		} else if (event.key === 'Enter') {
			event.preventDefault();
			const chosen = listShown && active >= 0 ? search.suggestions[active] : undefined;
			if (chosen) {
				pick(chosen);
				return;
			}
			listOpen = false;
			if (search.input !== search.query) {
				clearTimeout(timer);
				void search.start(search.input, tab.currentPage, { go: 'reveal', smooth: searchGlides() });
			} else {
				search.step(event.shiftKey ? -1 : 1, { smooth: searchGlides() });
			}
		} else if (event.key === 'Escape') {
			event.preventDefault();
			event.stopPropagation();
			if (listShown) {
				listOpen = false;
				active = -1;
			} else close();
		}
	}

	function step(direction: 1 | -1) {
		listOpen = false;
		search.step(direction, { smooth: searchGlides() });
	}

	// The field keeps its text, which is searched again when the bar opens again.
	function close() {
		search.open = false;
		search.clear();
		search.query = '';
		tab.viewer?.focus();
	}

	const status = $derived.by(() => {
		if (!search.query) return '';
		if (search.hits.length === 0) return search.running ? 'Searching…' : 'No results';
		const position = search.current >= 0 ? `${search.current + 1} of ${search.hits.length}` : `${search.hits.length}`;
		return search.running ? `${position}…` : position;
	});
</script>

<div
	class="absolute z-10 rounded-panel border border-line bg-surface-raised p-1 shadow-[0_4px_12px_var(--color-page-shadow)]"
	style:top="{app.overlayTop}px"
	style:right="{app.overlayRight}px"
	role="search"
	in:fly={{ y: -8, duration: motionMs(140), easing: cubicOut }}
	out:fly={{ y: -8, duration: motionMs(100), easing: cubicOut }}
>
	<div class="flex items-center gap-1">
		<input
			bind:this={input}
			bind:value={search.input}
			oninput={onInput}
			onkeydown={onKeyDown}
			onfocus={() => (focused = true)}
			onblur={() => {
				focused = false;
				listOpen = false;
				active = -1;
			}}
			class="field h-8 w-56"
			placeholder="Find in document"
			aria-label="Find in document"
			role="combobox"
			aria-autocomplete="list"
			aria-expanded={listShown}
			aria-controls={LIST_ID}
			aria-activedescendant={listShown && active >= 0 ? `${LIST_ID}-${active}` : undefined}
			data-search-input
		/>
		<span class="min-w-20 px-1 text-center text-sm text-fg-muted tabular-nums" aria-live="polite">
			{status}
		</span>
		<button
			type="button"
			class="icon-button"
			aria-label="Previous match (Shift+Enter)"
			data-tip="Previous match (Shift+Enter)"
			disabled={search.hits.length === 0}
			onclick={() => step(-1)}
		>
			<ChevronUp size={16} aria-hidden="true" />
		</button>
		<button
			type="button"
			class="icon-button"
			aria-label="Next match (Enter)"
			data-tip="Next match (Enter)"
			disabled={search.hits.length === 0}
			onclick={() => step(1)}
		>
			<ChevronDown size={16} aria-hidden="true" />
		</button>
		<button type="button" class="icon-button" aria-label="Close find (Esc)" data-tip="Close (Esc)" onclick={close}>
			<X size={16} aria-hidden="true" />
		</button>
	</div>
	<!-- Pressed, a suggestion keeps the focus in the field: the list goes with it. -->
	<div
		id={LIST_ID}
		class="menu-content absolute top-full right-0 left-0 mt-1 min-w-0"
		class:hidden={!listShown}
		role="listbox"
		aria-label="Words found"
		tabindex="-1"
		onpointerdown={(event) => event.preventDefault()}
		data-search-suggestions
	>
		{#if listShown}
			{#each search.suggestions as s, i (s.text)}
				{@const [before, match, after] = splitMatch(s.text, search.query)}
				<!-- The keys reach the suggestions through the field (aria-activedescendant). -->
				<!-- svelte-ignore a11y_click_events_have_key_events -->
				<div
					id="{LIST_ID}-{i}"
					class="menu-item cursor-pointer"
					role="option"
					aria-selected={i === active}
					tabindex="-1"
					data-highlighted={i === active ? '' : undefined}
					onpointermove={() => (active = i)}
					onclick={() => pick(s)}
				>
					<span class="min-w-0 truncate">{before}<span class="font-semibold">{match}</span>{after}</span>
					<span class="menu-shortcut tabular-nums" aria-label="{s.count} {s.count === 1 ? 'match' : 'matches'}">{s.count}</span>
				</div>
			{/each}
		{/if}
	</div>
</div>
