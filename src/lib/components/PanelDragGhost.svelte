<script lang="ts">
	// The panel being dragged between the side panes, following the pointer.
	import { PANEL_INFO, panelDrag } from './panels.svelte.ts';

	const drag = $derived(panelDrag.current?.moving ? panelDrag.current : null);

	$effect(() => {
		document.documentElement.classList.toggle('panel-dragging', drag !== null);
	});
</script>

{#if drag}
	{@const info = PANEL_INFO[drag.panel]}
	{@const Icon = info.icon}
	<div class="panel-drag-ghost" style:left="{drag.x + 12}px" style:top="{drag.y + 12}px" aria-hidden="true">
		<Icon size={16} />
		{info.label}
	</div>
{/if}
