<script lang="ts">
	// About (Help menu): version, license, and where the source code is (AGPL-3.0
	// section 6). Lectrix makes no network requests, so the address is shown and copied, not
	// opened.
	import { getVersion } from '@tauri-apps/api/app';
	import { Dialog } from 'bits-ui';

	import iconUrl from '#lib/assets/lectrix-icon.svg';

	import { APP_NAME, SOURCE_URL } from '#lib/config.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	let version = $state('');

	$effect(() => {
		if (!app.aboutOpen || version) return;
		void getVersion()
			.then((v) => (version = v))
			.catch(() => {});
	});

	async function copy() {
		try {
			await navigator.clipboard.writeText(SOURCE_URL);
			app.notify({ kind: 'info', message: 'The source code address is on the clipboard.' }, 4000);
		} catch {
			app.notify({ kind: 'error', message: 'The address couldn’t be copied to the clipboard.', suggestion: 'Select it and press Ctrl+C.' });
		}
	}
</script>

<Dialog.Root bind:open={app.aboutOpen}>
	<Dialog.Portal>
		<Dialog.Overlay class="dialog-overlay" />
		<Dialog.Content class="dialog-content w-[440px]">
			<div class="flex items-center gap-4">
				<!-- Decorative: the title names the app. -->
				<img src={iconUrl} alt="" width="56" height="56" draggable="false" />
				<div>
					<Dialog.Title class="text-base font-semibold">About {APP_NAME}</Dialog.Title>
					<Dialog.Description class="mt-1 text-sm text-fg-muted">
						{version ? `Version ${version}` : ''}
					</Dialog.Description>
				</div>
			</div>
			<div class="mt-4 flex flex-col gap-3 text-sm">
				<p>
					{APP_NAME} is free software: you can redistribute it and change it under the terms of the GNU Affero
					General Public License, version 3 or later. It comes with no warranty.
				</p>
				<p>It is built on MuPDF by Artifex Software, also under the GNU AGPL.</p>
				<div class="flex flex-col gap-1">
					<span id="about-source-label">Source code</span>
					<div class="flex gap-2">
						<input
							class="field h-8 min-w-0 flex-1 select-text"
							readonly
							value={SOURCE_URL}
							aria-labelledby="about-source-label"
							onfocus={(e) => e.currentTarget.select()}
						/>
						<button type="button" class="button" onclick={() => void copy()}>Copy</button>
					</div>
				</div>
				<p class="text-xs text-fg-muted">
					The license and the licenses of the components {APP_NAME} includes are in its installation folder:
					LICENSE.txt, THIRD_PARTY_NOTICES.md and THIRD_PARTY_LICENSES.md.
				</p>
			</div>
			<div class="mt-6 flex justify-end">
				<Dialog.Close class="button button-primary">Close</Dialog.Close>
			</div>
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
