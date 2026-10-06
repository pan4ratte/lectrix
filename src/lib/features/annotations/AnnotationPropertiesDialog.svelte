<script lang="ts">
	// The Properties dialog of the selected annotation (section 6.5), from the page's context
	// menu or the bar of a read-only annotation: its author, which Save changes (one undo
	// step), and when it was created and last changed.
	import { Dialog } from 'bits-ui';
	import { untrack } from 'svelte';

	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { update } from './actions.ts';
	import { capabilities, formatDate, typeName } from './tools.ts';

	let { tab }: { tab: DocTab } = $props();

	const a = $derived(tab.selectedAnnotationInfo);
	const editable = $derived(a ? capabilities(a, tab.flags.canAnnotate).text : false);

	let author = $state('');

	$effect(() => {
		if (!app.annotationPropertiesOpen) return;
		if (!a) app.annotationPropertiesOpen = false;
	});

	// The field starts from the annotation each time the dialog opens.
	$effect(() => {
		if (app.annotationPropertiesOpen) author = untrack(() => a?.author ?? '');
	});

	function save(event: SubmitEvent) {
		event.preventDefault();
		const name = author.trim();
		if (a && editable && name && name !== a.author) void update(tab, a.page, a.id, { author: name });
		app.annotationPropertiesOpen = false;
	}
</script>

<Dialog.Root bind:open={app.annotationPropertiesOpen}>
	<Dialog.Portal>
		<Dialog.Overlay class="dialog-overlay" />
		<Dialog.Content class="dialog-content w-[360px]" onCloseAutoFocus={() => tab.viewer?.focus()}>
			{#if a}
				<Dialog.Title class="text-base font-semibold">{typeName(a.subtype)} properties</Dialog.Title>
				<form class="mt-4 flex flex-col gap-4" onsubmit={save}>
					<label class="flex flex-col gap-1">
						<span class="text-sm">Author</span>
						<input class="field h-8" bind:value={author} readonly={!editable} maxlength="200" />
					</label>
					<dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm">
						<dt class="text-fg-muted">Created</dt>
						<dd>{formatDate(a.created)}</dd>
						<dt class="text-fg-muted">Modified</dt>
						<dd>{formatDate(a.modified)}</dd>
					</dl>
					<div class="mt-2 flex justify-end gap-2">
						{#if editable}
							<button type="submit" class="button button-primary">Save</button>
							<Dialog.Close class="button">Cancel</Dialog.Close>
						{:else}
							<Dialog.Close class="button button-primary">Close</Dialog.Close>
						{/if}
					</div>
				</form>
			{/if}
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
