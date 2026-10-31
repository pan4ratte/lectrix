<script lang="ts">
	// The stroke control of a drawing in its bar, its comment panel and its row in the
	// annotation list (section 6.5): a button showing the stroke width, which opens a panel
	// with the width on a slider. (The tools' own widths are in their panels, ToolOptions.)
	import { ChevronDown } from '@lucide/svelte';
	import { Popover } from 'bits-ui';

	import { menuOffset } from './bars.ts';
	import StrokeSettings, { pt } from './StrokeSettings.svelte';

	interface Props {
		/** Points. */
		width: number;
		onwidth: (width: number) => void;
	}

	let { width, onwidth }: Props = $props();

	let open = $state(false);

	// Esc closes the panel and nothing else: the window's own Esc (the selection, the tool)
	// is not for a panel that is open.
	function onContentKey(event: KeyboardEvent) {
		if (event.key !== 'Escape') return;
		event.preventDefault();
		event.stopPropagation();
		open = false;
	}
</script>

<Popover.Root bind:open>
	<Popover.Trigger
		class="dropdown toolbar-dropdown"
		aria-label="Stroke width: {pt(width)}"
		data-tip={open ? undefined : 'Stroke width'}
		data-stroke-button
	>
		<span class="min-w-0 flex-1 truncate text-left">{pt(width)}</span>
		<ChevronDown size={14} aria-hidden="true" class="shrink-0 text-fg-muted" />
	</Popover.Trigger>
	<Popover.Portal>
		<Popover.Content
			class="adjust-panel"
			side="bottom"
			align="center"
			sideOffset={menuOffset('bottom', 6)}
			escapeKeydownBehavior="ignore"
			role="dialog"
			aria-label="Stroke"
			onkeydown={onContentKey}
		>
			<StrokeSettings {width} {onwidth} />
		</Popover.Content>
	</Popover.Portal>
</Popover.Root>
