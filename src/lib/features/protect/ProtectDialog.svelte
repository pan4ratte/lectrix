<script lang="ts">
	// The Protect dialog (section 6.9, ADR 0026): a password to open the file, restrictions
	// on printing, changes and copying with the permissions password that lifts them, or
	// both. The same for a file (the tool, which then asks where to save) and for a document
	// in a tab (whose next save writes it). A document protected already, or with a change
	// waiting, can have its protection removed: with nothing ticked, the main button says so.
	import { Dialog } from 'bits-ui';
	import { untrack } from 'svelte';

	import Dropdown from '#lib/components/Dropdown.svelte';
	import { held } from '#lib/components/held.svelte.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	import PasswordField from './PasswordField.svelte';
	import { CHANGES, PRINTING, SIGNED_WARNING, draftInput, draftProblems, initialDraft, type ProtectDraft } from './protect.ts';

	/** The last request, which the dialog goes on showing while it closes. */
	const last = held(() => app.protectRequest);
	const request = $derived(last.current);
	const asking = $derived(app.protectRequest !== null);

	let draft = $state<ProtectDraft>(initialDraft(null, null));

	// A new request: its file's or document's protection, passwords empty.
	$effect(() => {
		const current = app.protectRequest;
		if (!current) return;
		untrack(() => {
			draft = initialDraft(current.protection, current.pending);
		});
	});

	const problems = $derived(draftProblems(draft));
	const input = $derived(draftInput(draft));
	/** Nothing ticked, for a document that has protection to remove. */
	const removing = $derived(!draft.requireOpen && !draft.restrict && (request?.canRemove ?? false));

	function finish(choice: Parameters<NonNullable<typeof app.protectRequest>['resolve']>[0]) {
		const current = app.protectRequest;
		if (!current) return;
		app.protectRequest = null;
		// The passwords go no further than the answer.
		draft = initialDraft(null, null);
		current.resolve(choice);
	}

	function submit(event: SubmitEvent) {
		event.preventDefault();
		if (removing) finish('remove');
		else if (input) finish(input);
	}
</script>

<Dialog.Root
	open={asking}
	onOpenChange={(open) => {
		if (!open) finish(null);
	}}
>
	<Dialog.Portal>
		<Dialog.Overlay class="dialog-overlay" />
		<Dialog.Content class="dialog-content w-[480px]" data-protect-dialog>
			{#if request}
				<Dialog.Title class="text-base font-semibold">Protect {request.name}</Dialog.Title>
				<Dialog.Description class="mt-1 text-sm text-fg-muted">
					A password to open the file, restrictions on what can be done with it, or both. They replace any protection it
					has.
				</Dialog.Description>
				<form class="mt-4 flex flex-col gap-4" onsubmit={submit}>
					<fieldset class="flex flex-col gap-2">
						<label class="flex items-center gap-2 font-medium">
							<input type="checkbox" bind:checked={draft.requireOpen} data-require-open />
							Require a password to open the file
						</label>
						<div class="protect-fields">
							<label class="text-sm" for="protect-open" class:opacity-50={!draft.requireOpen}>Password</label>
							<PasswordField
								id="protect-open"
								name="open"
								bind:value={draft.openPassword}
								disabled={!draft.requireOpen}
								invalid={problems.open !== null && problems.open !== 'The passwords don’t match.'}
							/>
							<label class="text-sm" for="protect-open-confirm" class:opacity-50={!draft.requireOpen}>Confirm password</label>
							<PasswordField
								id="protect-open-confirm"
								name="open-confirm"
								bind:value={draft.openConfirm}
								disabled={!draft.requireOpen}
								invalid={problems.open === 'The passwords don’t match.'}
							/>
							{#if problems.open}
								<p class="col-start-2 text-sm text-danger" role="alert">{problems.open}</p>
							{/if}
						</div>
					</fieldset>
					<fieldset class="flex flex-col gap-2">
						<label class="flex items-center gap-2 font-medium">
							<input type="checkbox" bind:checked={draft.restrict} data-restrict />
							Restrict printing, changes and copying
						</label>
						<div class="protect-fields">
							<label class="text-sm" for="protect-permissions" class:opacity-50={!draft.restrict}>Permissions password</label>
							<PasswordField
								id="protect-permissions"
								name="permissions"
								bind:value={draft.permissionsPassword}
								disabled={!draft.restrict}
								invalid={problems.permissions !== null && problems.permissions !== 'The passwords don’t match.'}
							/>
							<label class="text-sm" for="protect-permissions-confirm" class:opacity-50={!draft.restrict}>
								Confirm password
							</label>
							<PasswordField
								id="protect-permissions-confirm"
								name="permissions-confirm"
								bind:value={draft.permissionsConfirm}
								disabled={!draft.restrict}
								invalid={problems.permissions === 'The passwords don’t match.'}
							/>
							{#if problems.permissions}
								<p class="col-start-2 text-sm text-danger" role="alert">{problems.permissions}</p>
							{/if}
							<span class="text-sm" class:opacity-50={!draft.restrict} aria-hidden="true">Printing allowed</span>
							<Dropdown
								class="h-7 w-full"
								bind:value={draft.restrictions.printing}
								label="Printing allowed"
								disabled={!draft.restrict}
								options={PRINTING}
							/>
							<span class="text-sm" class:opacity-50={!draft.restrict} aria-hidden="true">Changes allowed</span>
							<Dropdown
								class="h-7 w-full"
								bind:value={draft.restrictions.changes}
								label="Changes allowed"
								disabled={!draft.restrict}
								options={CHANGES}
							/>
							<label class="col-start-2 flex items-center gap-2 text-sm" class:opacity-50={!draft.restrict}>
								<input type="checkbox" bind:checked={draft.restrictions.copying} disabled={!draft.restrict} />
								Allow copying of text, images and other content
							</label>
						</div>
					</fieldset>
					{#if request.signedWarning}
						<p class="text-sm text-fg-muted">{SIGNED_WARNING}</p>
					{/if}
					<div class="flex justify-end gap-2">
						<button type="button" class="button" onclick={() => finish(null)}>Cancel</button>
						<button type="submit" class="button button-primary" disabled={!removing && input === null}>
							{removing ? 'Remove protection' : 'Protect'}
						</button>
					</div>
				</form>
			{/if}
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
