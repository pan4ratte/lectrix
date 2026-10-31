<script lang="ts">
	// A tool's button in the annotation toolbar that also opens the tool's options (section
	// 6.5): the Pen's (its stroke) and the Shapes tool's (the shape, then its stroke). A click
	// on it picks the tool; a click on it once the tool is picked opens the panel, and closes
	// it again. A mark in its corner says it has options. The panel stays open while it is
	// used; Esc, a click elsewhere or another tool closes it.
	import { Popover } from 'bits-ui';
	import type { Component, Snippet } from 'svelte';

	import { menuOffset, type MenuSide } from './bars.ts';

	interface Props {
		/** The tool's name: the button's accessible name and the panel's. */
		label: string;
		icon: Component<{ size?: number; 'aria-hidden'?: boolean | 'true' }>;
		/** The tool is the one in use. */
		active: boolean;
		/** Annotating is not allowed here: the button looks unavailable (a click says why). */
		locked: boolean;
		/** Its tooltip while the tool is not picked ("Pen (P)"), and while it is. */
		tip: string;
		activeTip: string;
		/** Picks the tool. */
		onpick: () => void;
		/** Where the panel opens: below the toolbar, or beside it (a stacked toolbar). */
		side?: MenuSide;
		children: Snippet;
	}

	let { label, icon: Icon, active, locked, tip, activeTip, onpick, side = 'bottom', children }: Props = $props();

	let open = $state(false);
	let content = $state<HTMLElement | null>(null);

	// Another tool picked (from the keyboard, say) closes the panel.
	$effect(() => {
		if (!active) open = false;
	});

	/** While the tool is not picked, a click (or Enter, or Space) picks it and the panel
	 * stays shut: cancelling the event keeps the popover from toggling. */
	function onclick(event: MouseEvent) {
		if (active) return;
		event.preventDefault();
		onpick();
	}

	function onkeydown(event: KeyboardEvent) {
		if (active || (event.key !== 'Enter' && event.key !== ' ')) return;
		event.preventDefault();
		onpick();
	}

	/** Opened, the panel puts the focus on what is chosen in it (a checked radio), else on
	 * its first control. */
	function focusChosen(event: Event) {
		event.preventDefault();
		const chosen = content?.querySelector<HTMLElement>('[aria-checked="true"]');
		(chosen ?? content?.querySelector<HTMLElement>('input, button'))?.focus();
	}

	// Esc closes the panel and nothing else: the window's own Esc picks the Select tool.
	function onContentKey(event: KeyboardEvent) {
		if (event.key !== 'Escape') return;
		event.preventDefault();
		event.stopPropagation();
		open = false;
	}
</script>

<Popover.Root bind:open>
	<Popover.Trigger
		class="icon-button tool-button tool-button-more"
		aria-pressed={active}
		aria-label={label}
		aria-disabled={locked}
		data-tip={open ? undefined : active ? activeTip : tip}
		data-tool-options={label}
		{onclick}
		{onkeydown}
	>
		<Icon size={18} aria-hidden="true" />
	</Popover.Trigger>
	<Popover.Portal>
		<Popover.Content
			bind:ref={content}
			class="adjust-panel tool-options"
			{side}
			align={side === 'bottom' ? 'center' : 'start'}
			sideOffset={menuOffset(side, 6)}
			escapeKeydownBehavior="ignore"
			role="dialog"
			aria-label={label}
			onkeydown={onContentKey}
			onOpenAutoFocus={focusChosen}
		>
			{@render children()}
		</Popover.Content>
	</Popover.Portal>
</Popover.Root>
