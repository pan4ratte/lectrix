<script lang="ts">
	// Shows or hides a side pane (section 8). It floats at the window's edge, at the outer end
	// of the pane's panel row, and stays in that place, in the view bar's row, while the pane
	// is closed, as in Obsidian. A closed pane's button also takes panels dropped on it.
	import { PanelLeft, PanelRight } from '@lucide/svelte';

	import { app } from '#lib/stores/app.svelte.ts';

	import { dropSlot } from './panels.svelte.ts';
	import type { PaneSide } from './panes.ts';

	let { side }: { side: PaneSide } = $props();

	const open = $derived(app.isOpen(side));
	const name = $derived(side === 'left' ? 'left pane' : 'right pane');
	/** Some annotation needs repair and the closed pane holds the annotation list, whose
	 * tab carries the badge while the pane is open. */
	const needing = $derived(
		!open &&
			app.panels[side].includes('annotations') &&
			(app.active?.allAnnotations.some((a) => a.problems.length > 0) ?? false)
	);
	const target = $derived(!open && dropSlot(side) !== null);
</script>

<button
	type="button"
	class="icon-button relative shrink-0"
	class:pane-drop-target={target}
	aria-label={open ? `Hide ${name}` : `Show ${name}`}
	aria-pressed={open}
	aria-describedby={needing ? `${side}-pane-needs-repair` : undefined}
	title={open ? `Hide ${name}` : `Show ${name}`}
	data-panel-drop={open ? undefined : side}
	onclick={() => app.togglePane(side)}
>
	{#if side === 'left'}
		<PanelLeft size={16} aria-hidden="true" />
	{:else}
		<PanelRight size={16} aria-hidden="true" />
	{/if}
	{#if needing}
		<span class="absolute top-1 right-1 h-2 w-2 rounded-full bg-danger" aria-hidden="true"></span>
		<span id="{side}-pane-needs-repair" class="sr-only">Some annotations need repair</span>
	{/if}
</button>
