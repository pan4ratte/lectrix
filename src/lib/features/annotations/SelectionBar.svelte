<script lang="ts">
	// The bar over selected text (section 6.5): the colour every tool draws with, so it can be
	// picked before the text is marked, then the quick tools chosen in Settings. Markup uses
	// that colour and each tool's last opacity; "Highlight with note" opens the inspector on
	// the note. Copy shows a tick for a moment, then the bar closes and the text stays
	// selected.
	import {
		BookmarkPlus,
		Check,
		Copy,
		Highlighter,
		MessageSquarePlus,
		Strikethrough,
		Underline,
		ZodiacAquarius
	} from '@lucide/svelte';
	import { onDestroy, type Component } from 'svelte';

	import { addBookmark } from '#lib/features/bookmarks/actions.ts';
	import { copySelection } from '#lib/features/viewer/actions.ts';
	import type { QuickTool } from '#lib/ipc/index.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { markSelection, markupOpensComment } from './actions.ts';
	import { quickToolAllowed, type Area } from './bars.ts';
	import ColorPicker from './ColorPicker.svelte';
	import FloatingBar from './FloatingBar.svelte';
	import { tools } from './state.svelte.ts';
	import { QUICK_TOOLS } from './tools.ts';

	interface Props {
		tab: DocTab;
		/** What the bar belongs to (where the pointer was released, or the selection's last
		 * line), and the side it goes on when both fit. */
		anchor: Area;
		prefer: 'above' | 'below';
		view: Area;
		contentWidth: number;
		/** The tools to show, in Settings' order. */
		chosen: readonly QuickTool[];
		/** Closes the bar of selection `sel`, which stays selected. */
		onclose: (sel: NonNullable<DocTab['selection']>) => void;
	}

	let { tab, anchor, prefer, view, contentWidth, chosen, onclose }: Props = $props();

	/** How long the tick shows after Copy before the bar closes. */
	const COPIED_MS = 600;
	let copied = $state(false);
	let closing = 0;
	onDestroy(() => clearTimeout(closing));
	// Another selection gets the bar as it is.
	$effect(() => {
		void tab.selection;
		copied = false;
		clearTimeout(closing);
	});

	const ICONS: Record<QuickTool, Component<{ size?: number; 'aria-hidden'?: boolean | 'true' }>> = {
		highlight: Highlighter,
		underline: Underline,
		strikeOut: Strikethrough,
		squiggly: ZodiacAquarius,
		highlightNote: MessageSquarePlus,
		copy: Copy,
		bookmark: BookmarkPlus
	};

	const shown = $derived(QUICK_TOOLS.filter((t) => chosen.includes(t.id)));

	async function run(tool: QuickTool) {
		switch (tool) {
			case 'copy': {
				const sel = tab.selection;
				if (!sel || !(await copySelection(tab))) return;
				copied = true;
				tab.viewer?.focus();
				closing = window.setTimeout(() => onclose(sel), COPIED_MS);
				return;
			}
			case 'bookmark':
				await addBookmark(tab);
				return;
			case 'highlightNote':
				await markSelection(tab, 'highlight', true);
				return;
			default:
				await markSelection(tab, tool);
				if (!markupOpensComment()) tab.viewer?.focus();
		}
	}
</script>

<FloatingBar label="Quick tools" {anchor} {view} {contentWidth} {prefer}>
	<ColorPicker color={tools.color} opacity={null} label="Colour" oncolor={(color) => tools.setColor(color)} />
	<span class="mx-1 h-6 w-px shrink-0 bg-line" aria-hidden="true"></span>
	{#each shown as t (t.id)}
		{@const Icon = ICONS[t.id]}
		{@const done = t.id === 'copy' && copied}
		<button
			type="button"
			class="icon-button"
			aria-label={done ? 'Copied' : t.label}
			data-tip={done ? 'Copied' : t.label}
			disabled={!quickToolAllowed(t.id, tab.flags, tab.canEditBookmarks)}
			onclick={() => void run(t.id)}
		>
			{#if done}
				<Check size={18} class="copied-tick" aria-hidden="true" />
			{:else}
				<Icon size={18} aria-hidden="true" />
			{/if}
		</button>
	{/each}
</FloatingBar>
