<script lang="ts">
	// The bar over selected text (section 6.5): the quick tools chosen in Settings. Markup
	// uses each tool's last style; "Highlight with note" opens the inspector on the note.
	import {
		BookmarkPlus,
		Copy,
		Highlighter,
		MessageSquarePlus,
		Spline,
		Strikethrough,
		Underline
	} from '@lucide/svelte';
	import type { Component } from 'svelte';

	import { addBookmark } from '#lib/features/bookmarks/actions.ts';
	import { copySelection } from '#lib/features/viewer/actions.ts';
	import type { QuickTool } from '#lib/ipc/index.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { markSelection, openInspector } from './actions.ts';
	import { quickToolAllowed, type Area } from './bars.ts';
	import FloatingBar from './FloatingBar.svelte';
	import { QUICK_TOOLS } from './tools.ts';

	interface Props {
		tab: DocTab;
		/** The selected line the bar belongs to, and which way the selection went. */
		anchor: Area;
		forward: boolean;
		view: Area;
		contentWidth: number;
		/** The tools to show, in Settings' order. */
		chosen: readonly QuickTool[];
	}

	let { tab, anchor, forward, view, contentWidth, chosen }: Props = $props();

	const ICONS: Record<QuickTool, Component<{ size?: number; 'aria-hidden'?: boolean | 'true' }>> = {
		highlight: Highlighter,
		underline: Underline,
		strikeOut: Strikethrough,
		squiggly: Spline,
		highlightNote: MessageSquarePlus,
		copy: Copy,
		bookmark: BookmarkPlus
	};

	const shown = $derived(QUICK_TOOLS.filter((t) => chosen.includes(t.id)));

	async function run(tool: QuickTool) {
		switch (tool) {
			case 'copy':
				await copySelection(tab);
				return;
			case 'bookmark':
				await addBookmark(tab);
				return;
			case 'highlightNote':
				if (await markSelection(tab, 'highlight')) openInspector(true);
				return;
			default:
				await markSelection(tab, tool);
				tab.viewer?.focus();
		}
	}
</script>

<FloatingBar label="Quick tools" {anchor} {view} {contentWidth} prefer={forward ? 'below' : 'above'}>
	{#each shown as t (t.id)}
		{@const Icon = ICONS[t.id]}
		<button
			type="button"
			class="icon-button"
			aria-label={t.label}
			title={t.label}
			disabled={!quickToolAllowed(t.id, tab.flags, tab.canEditBookmarks)}
			onclick={() => void run(t.id)}
		>
			<Icon size={18} aria-hidden="true" />
		</button>
	{/each}
</FloatingBar>
