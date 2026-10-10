<script lang="ts">
	// What's new (section 6.6). From the Help menu and after an update: the changelog of the
	// version running, newest version first, each version's changes under the parts of the
	// app they touch, in the app's language where the changelog is translated. From the
	// update notice: the changes the update brings (latest.json carries the latest
	// versions' changelog), with Update at the bottom.
	import { X } from '@lucide/svelte';
	import { Dialog } from 'bits-ui';

	import { APP_NAME, LOCALE } from '#lib/config.ts';
	import { update } from '#lib/features/update/update.svelte.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	import { changesSince, hasChanges, loadChangelog, type Release, type Span } from './changelog.ts';

	let installed = $state<Release[] | null>(null);
	let failed = $state(false);
	let body: HTMLElement | null = $state(null);

	const incoming = $derived(app.whatsNewUpdate);
	const releases = $derived(incoming ? changesSince(incoming.notes, incoming.currentVersion) : installed);
	/** Update is offered while the notice still offers this version. */
	const canUpdate = $derived(
		incoming !== null && update.stage.kind === 'available' && update.stage.info.version === incoming.version
	);

	$effect(() => {
		if (!app.whatsNewOpen || incoming || installed) return;
		loadChangelog(LOCALE)
			.then((all) => (installed = all.filter(hasChanges)))
			.catch(() => (failed = true));
	});

	/** The list takes the focus, so the keys scroll it at once; Tab then reaches the buttons. */
	function focusList(event: Event) {
		event.preventDefault();
		body?.focus({ preventScroll: true });
	}

	function startUpdate() {
		app.whatsNewOpen = false;
		void update.update();
	}
</script>

{#snippet text(line: Span[])}
	{#each line as span, i (i)}
		{#if span.bold}<strong>{span.text}</strong>{:else}{span.text}{/if}
	{/each}
{/snippet}

<Dialog.Root bind:open={app.whatsNewOpen}>
	<Dialog.Portal>
		<Dialog.Overlay class="dialog-overlay" />
		<Dialog.Content class="dialog-content whats-new" onOpenAutoFocus={focusList}>
			<div class="whats-new-title">
				{#if incoming}
					<Dialog.Title class="whats-new-heading">What’s new in {APP_NAME} {incoming.version}</Dialog.Title>
					<Dialog.Description class="mt-1 text-sm text-fg-muted">
						Changes since your version, {incoming.currentVersion}.
					</Dialog.Description>
				{:else}
					<Dialog.Title class="whats-new-heading">What’s new in {APP_NAME}</Dialog.Title>
				{/if}
			</div>
			<!-- Focusable so the keyboard can scroll it (a scrolling region). -->
			<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
			<div bind:this={body} class="whats-new-body" tabindex="0" role="region" aria-label="Changes by version">
				{#if failed && !incoming}
					<p class="text-fg-muted">The list of changes could not be read.</p>
				{:else if releases}
					{#each releases as release (release.version)}
						<section class="whats-new-release">
							<h3>{release.version ? `Version ${release.version}` : 'Not released yet'}</h3>
							{#each release.intro as line, i (i)}
								<p class="whats-new-intro">{@render text(line)}</p>
							{/each}
							{#each release.sections as section (section.title)}
								<h4>{section.title}</h4>
								<ul>
									{#each section.items as item, i (i)}
										<li>{@render text(item)}</li>
									{/each}
								</ul>
							{/each}
						</section>
					{/each}
				{/if}
			</div>
			{#if canUpdate}
				<div class="whats-new-footer">
					<button type="button" class="button button-primary" onclick={startUpdate}>Update</button>
				</div>
			{/if}
			<Dialog.Close class="icon-button dialog-close" aria-label="Close">
				<X size={16} aria-hidden="true" />
			</Dialog.Close>
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
