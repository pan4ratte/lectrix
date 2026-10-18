<script lang="ts">
	// What a side panel shows with nothing to list (the bookmarks, the annotation list): its
	// icon in a soft circle, a short title, a line on what to do, and a button for it where
	// there is one. A third of the way down the panel, so it reads as the panel's content
	// rather than a note under its header.
	import type { Component, Snippet } from 'svelte';

	interface Props {
		icon: Component<{ size?: number; 'aria-hidden'?: boolean | 'true' }>;
		heading: string;
		/** The line under the title. */
		children?: Snippet;
		/** A button for what the line suggests. */
		action?: Snippet;
	}

	let { icon: Icon, heading, children, action }: Props = $props();
</script>

<div class="panel-empty">
	<div class="panel-empty-body">
		<span class="panel-empty-icon"><Icon size={20} aria-hidden="true" /></span>
		<p class="panel-empty-title">{heading}</p>
		{#if children}
			<p class="panel-empty-line">{@render children()}</p>
		{/if}
		{#if action}
			<div class="panel-empty-action">{@render action()}</div>
		{/if}
	</div>
</div>
