<script lang="ts">
	// About (Help menu): the icon, the name, the version, its author (a link to their GitHub
	// profile), the license, and a button that opens the source code's page on GitHub
	// (AGPL-3.0 section 6). Both open in the default browser: Rust opens them, from
	// addresses it holds itself, and the webview stays offline (section 2).
	import { X } from '@lucide/svelte';
	import { getVersion } from '@tauri-apps/api/app';
	import { Dialog } from 'bits-ui';

	import iconUrl from '#lib/assets/lectrix-icon.svg';

	import { APP_NAME, AUTHOR, AUTHOR_URL, SOURCE_URL } from '#lib/config.ts';
	import { openWebPage, toAppError, type WebPage } from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	let version = $state('');
	let content: HTMLElement | null = $state(null);

	$effect(() => {
		if (!app.aboutOpen || version) return;
		void getVersion()
			.then((v) => (version = v))
			.catch(() => {});
	});

	/** The dialog itself takes the focus, so no control looks picked as it opens; Tab
	 * then reaches the author, the GitHub button and Close. */
	function focusDialog(event: Event) {
		event.preventDefault();
		content?.focus();
	}

	async function open(page: WebPage) {
		try {
			await openWebPage(page);
		} catch (e) {
			app.showError(toAppError(e));
		}
	}
</script>

<Dialog.Root bind:open={app.aboutOpen}>
	<Dialog.Portal>
		<Dialog.Overlay class="dialog-overlay" />
		<Dialog.Content
			bind:ref={content}
			class="dialog-content w-[400px] outline-none"
			tabindex={-1}
			onOpenAutoFocus={focusDialog}
		>
			<div class="flex flex-col items-center text-center">
				<!-- Decorative: the title names the app. -->
				<img src={iconUrl} alt="" width="96" height="96" draggable="false" />
				<Dialog.Title class="mt-3 text-[20px] font-semibold">{APP_NAME}</Dialog.Title>
				<Dialog.Description class="mt-1 text-sm text-fg-muted">
					{version ? `Version ${version}` : ''}
				</Dialog.Description>
				<!-- A button, not a link: a link would navigate the webview, which never goes online. -->
				<p class="text-sm text-fg-muted">
					by <button type="button" class="text-link" title={AUTHOR_URL} onclick={() => void open('author')}>{AUTHOR}</button>
				</p>
				<p class="mt-5 text-sm text-balance">
					{APP_NAME} is free software: you can redistribute it and change it under the terms of the GNU Affero General
					Public License, version 3 or later. It comes with no warranty. It is built on MuPDF by Artifex Software, also
					under the GNU AGPL.
				</p>
				<button type="button" class="button mt-5 gap-2" title={SOURCE_URL} onclick={() => void open('source')}>
					<!-- The GitHub mark from Simple Icons 16.34.0 (CC0-1.0). -->
					<svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
						<path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12" />
					</svg>
					Source code on GitHub
				</button>
			</div>
			<Dialog.Close class="icon-button dialog-close" aria-label="Close">
				<X size={16} aria-hidden="true" />
			</Dialog.Close>
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
