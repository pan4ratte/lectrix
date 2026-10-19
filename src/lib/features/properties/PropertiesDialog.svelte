<script lang="ts">
	// A document's properties (section 6.10): its groups as tabs down the left side, as in
	// Settings. Description: its file, its title, author, subject and keywords (changed with
	// OK, as one undo step), and what it declares. Security: its protection and what that
	// allows, with Change protection… (section 6.9). Fonts: every font it uses, gathered a
	// few pages at a time when the tab is first shown, stopped when the dialog closes.
	import { FileText, Lock, TriangleAlert, Type } from '@lucide/svelte';
	import { Dialog, Tabs } from 'bits-ui';
	import { untrack } from 'svelte';

	import { toolMark } from '#lib/features/annotations/toolMark.ts';
	import { pendingKind, protectionKind } from '#lib/features/protect/protect.ts';
	import { protectDocument } from '#lib/features/protect/actions.ts';
	import { unlockDocument } from '#lib/features/unprotect/actions.ts';
	import { pdfaName } from '#lib/format.ts';
	import {
		getDocumentFonts,
		getDocumentProperties,
		toAppError,
		type DocumentDescription,
		type DocumentProperties,
		type UsedFont
	} from '#lib/ipc/index.ts';
	import { app, type PropertiesSection } from '#lib/stores/app.svelte.ts';

	import {
		allowedRows,
		cipherText,
		embeddingText,
		encodingText,
		fontKindText,
		fontNeedsAttention,
		mergeFonts,
		pageSizeText,
		sameDescription,
		sizeText,
		trimmed,
		usesInches
	} from './properties.ts';

	const GROUPS: readonly { id: PropertiesSection; label: string; icon: typeof FileText }[] = [
		{ id: 'description', label: 'Description', icon: FileText },
		{ id: 'security', label: 'Security', icon: Lock },
		{ id: 'fonts', label: 'Fonts', icon: Type }
	];
	/** Pages whose fonts are asked for at a time. */
	const FONT_PAGES = 25;

	const request = $derived(app.propertiesRequest);
	const tab = $derived(request ? (app.tabs.find((t) => t.id === request.tabId) ?? null) : null);

	let section = $state<PropertiesSection>('description');
	let properties = $state<DocumentProperties | null>(null);
	let draft = $state<DocumentDescription>({ title: '', author: '', subject: '', keywords: '' });
	let fonts = $state<UsedFont[] | null>(null);
	/** The page the font search has reached, while it goes on. */
	let fontsAt = $state<number | null>(null);
	/** Each opening of the dialog: what an earlier one started stops. */
	let opening = 0;

	async function load(id: number) {
		const mine = opening;
		try {
			const read = await getDocumentProperties(id);
			if (mine !== opening) return;
			properties = read;
			draft = { ...read.description };
		} catch (e) {
			app.showError(toAppError(e));
		}
	}

	// A new request: read the properties afresh.
	$effect(() => {
		const current = request;
		if (!current) return;
		untrack(() => {
			opening++;
			section = current.section;
			properties = null;
			fonts = null;
			fontsAt = null;
			void load(current.tabId);
		});
	});

	// The Fonts tab, first shown: the fonts gathered a few pages at a time.
	$effect(() => {
		if (section !== 'fonts' || !request) return;
		untrack(() => {
			if (fonts === null && fontsAt === null) void gatherFonts(request.tabId);
		});
	});

	async function gatherFonts(id: number) {
		const mine = opening;
		const count = tab?.pageCount ?? 0;
		let found: UsedFont[] = [];
		try {
			for (let from = 0; from < count; from += FONT_PAGES) {
				fontsAt = from + 1;
				const more = await getDocumentFonts(id, from, Math.min(count, from + FONT_PAGES));
				if (mine !== opening) return;
				found = mergeFonts(found, more);
			}
			fonts = found;
		} catch (e) {
			if (mine === opening) app.showError(toAppError(e));
		} finally {
			if (mine === opening) fontsAt = null;
		}
	}

	/** The description can be changed: not while the document is open for reading only,
	 * nor when its security settings refuse changes. */
	const editable = $derived(tab !== null && !tab.readOnly && tab.flags.canAssemble);
	const changed = $derived(properties !== null && !sameDescription(draft, properties.description));

	function close() {
		opening++;
		app.propertiesRequest = null;
	}

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		const current = tab;
		if (current && editable && changed) {
			if (!(await app.apply(current, { kind: 'setDescription', description: trimmed(draft) }))) return;
		}
		close();
	}

	const page = $derived(tab ? tab.pages[Math.min(tab.currentPage, tab.pageCount - 1)] : undefined);
	const locale = typeof navigator === 'undefined' ? 'en-US' : navigator.language;
	const date = (ms: number | null) =>
		ms === null ? '—' : new Date(ms).toLocaleString(undefined, { dateStyle: 'long', timeStyle: 'short' });
	const pages = (n: number) => (n === 1 ? '1 page' : `${n.toLocaleString()} pages`);
	const attention = $derived(fonts?.some(fontNeedsAttention) ?? false);
</script>

<Dialog.Root
	open={request !== null && tab !== null}
	onOpenChange={(open) => {
		if (!open) close();
	}}
>
	<Dialog.Portal>
		<Dialog.Overlay class="dialog-overlay" />
		<Dialog.Content class="dialog-content settings-dialog properties-dialog" data-properties-dialog>
			{#if tab}
				<Tabs.Root bind:value={section} orientation="vertical" class="flex min-h-0 min-w-0 flex-1">
					<div class="settings-nav">
						<Dialog.Title class="settings-title truncate">Properties</Dialog.Title>
						<Tabs.List class="tool-group flex flex-col gap-0.5" aria-label="Properties" {@attach toolMark}>
							<span class="tool-mark settings-mark" aria-hidden="true"></span>
							{#each GROUPS as g (g.id)}
								<Tabs.Trigger value={g.id} class="settings-tab glide-item" data-properties-tab={g.id}>
									<g.icon size={16} aria-hidden="true" />
									<span class="truncate">{g.label}</span>
								</Tabs.Trigger>
							{/each}
						</Tabs.List>
					</div>
					<form class="properties-body" onsubmit={submit}>
						<Tabs.Content value="description" class="settings-page">
							<h2 class="settings-heading truncate">{tab.name}</h2>
							{#if properties}
								<dl class="properties-grid mt-3">
									{#if tab.untitled}
										<dt>File</dt>
										<dd>Not saved yet</dd>
									{:else}
										<dt>Folder</dt>
										<dd class="break-all">{properties.folder ?? '—'}</dd>
										<dt>Size</dt>
										<dd>{properties.fileSize === null ? '—' : sizeText(properties.fileSize)}</dd>
									{/if}
									<dt>PDF version</dt>
									<dd>{properties.version}</dd>
								</dl>
								<section class="settings-section mt-5" aria-label="Description">
									<h3 class="settings-section-title">Description</h3>
									<div class="properties-grid mt-2">
										<label for="properties-title">Title</label>
										<input id="properties-title" class="field h-7" bind:value={draft.title} readonly={!editable} />
										<label for="properties-author">Author</label>
										<input id="properties-author" class="field h-7" bind:value={draft.author} readonly={!editable} />
										<label for="properties-subject">Subject</label>
										<input id="properties-subject" class="field h-7" bind:value={draft.subject} readonly={!editable} />
										<label for="properties-keywords">Keywords</label>
										<input id="properties-keywords" class="field h-7" bind:value={draft.keywords} readonly={!editable} />
									</div>
									{#if tab.readOnly}
										<p class="setting-note mt-2">
											This file is open for reading only, because it is a PDF/A file. Choose Enable editing in the bar
											above the pages to change its description.
										</p>
									{:else if !tab.flags.canAssemble}
										<p class="setting-note mt-2 flex flex-wrap items-center gap-2">
											This document’s security settings don’t allow changing its description.
											<button
												type="button"
												class="button h-7"
												onclick={() => {
													const t = tab;
													close();
													if (t) void unlockDocument(t);
												}}
											>
												Unlock…
											</button>
										</p>
									{/if}
								</section>
								<section class="settings-section" aria-label="About the document">
									<h3 class="settings-section-title">About the document</h3>
									<dl class="properties-grid mt-2">
										<dt>Created</dt>
										<dd>{date(properties.created)}</dd>
										<dt>Modified</dt>
										<dd>{date(properties.modified)}</dd>
										<dt>Application</dt>
										<dd>{properties.creator ?? '—'}</dd>
										<dt>PDF producer</dt>
										<dd>{properties.producer ?? '—'}</dd>
										<dt>Pages</dt>
										<dd>{pages(tab.pageCount)}</dd>
										<dt>Page size</dt>
										<dd>{page ? pageSizeText(page.width, page.height, usesInches(locale)) : '—'}</dd>
										<dt>Tagged PDF</dt>
										<dd>{properties.tagged ? 'Yes' : 'No'}</dd>
										<dt>Fast web view</dt>
										<dd>{properties.linearized ? 'Yes' : 'No'}</dd>
										<dt>PDF/A</dt>
										<dd>{tab.fileFlags.pdfa ? `Claims ${pdfaName(tab.fileFlags.pdfa)}` : 'None'}</dd>
										<dt>Signatures</dt>
										<dd>{properties.signatures === 0 ? 'None' : properties.signatures.toLocaleString()}</dd>
									</dl>
								</section>
							{/if}
						</Tabs.Content>
						<Tabs.Content value="security" class="settings-page">
							<h2 class="settings-heading">Security</h2>
							{#if properties}
								{@const protection = properties.protection}
								<dl class="properties-grid mt-3">
									<dt>Protection</dt>
									<dd data-protection-kind>
										{protection ? protectionKind(protection.openPassword, protection.restricted) : 'None'}
									</dd>
									{#if protection}
										<dt>Encryption</dt>
										<dd>{cipherText(protection)}</dd>
									{/if}
									{#if tab.state.protection}
										<dt>When saved</dt>
										<dd data-protection-pending>{pendingKind(tab.state.protection)}</dd>
									{/if}
								</dl>
								{#if protection && !protection.restricted}
									<p class="setting-note mt-3">It restricts nothing once it is open.</p>
								{:else if protection}
									<section class="settings-section mt-5" aria-label="What it allows">
										<h3 class="settings-section-title">What it allows</h3>
										<dl class="properties-grid mt-2">
											{#each allowedRows(protection) as row (row.label)}
												<dt>{row.label}</dt>
												<dd>{row.value}</dd>
											{/each}
										</dl>
										{#if protection.unrestricted}
											<p class="setting-note mt-2">
												It was opened with its permissions password, so nothing is restricted while it is open in
												Lectrix.
											</p>
										{/if}
									</section>
								{/if}
								<div class="mt-5">
									<button
										type="button"
										class="button"
										disabled={tab.readOnly}
										onclick={async () => {
											const t = tab;
											if (!t) return;
											await protectDocument(t);
											// The protection shown is the file's: read it again.
											if (app.propertiesRequest?.tabId === t.id) void load(t.id);
										}}
									>
										Change protection…
									</button>
								</div>
							{/if}
						</Tabs.Content>
						<Tabs.Content value="fonts" class="settings-page">
							<h2 class="settings-heading">Fonts</h2>
							{#if fontsAt !== null}
								<p class="setting-note mt-3" role="status">
									Looking for fonts… page {fontsAt.toLocaleString()} of {tab.pageCount.toLocaleString()}
								</p>
							{:else if fonts && fonts.length === 0}
								<p class="setting-note mt-3">This document uses no fonts.</p>
							{:else if fonts}
								<table class="properties-fonts mt-3">
									<thead>
										<tr>
											<th scope="col">Font</th>
											<th scope="col">Type</th>
											<th scope="col">Embedded</th>
											<th scope="col">Encoding</th>
										</tr>
									</thead>
									<tbody>
										{#each fonts as font (`${font.id}:${font.name}`)}
											<tr data-font={font.name}>
												<td>
													<span class="flex items-center gap-1.5">
														{font.name}
														{#if fontNeedsAttention(font)}
															<span
																class="flex shrink-0 text-danger"
																role="img"
																aria-label="Not embedded"
																data-tip="Readers draw this text with a font of their own"
															>
																<TriangleAlert size={13} aria-hidden="true" />
															</span>
														{/if}
													</span>
												</td>
												<td>{fontKindText(font.kind)}</td>
												<td>{embeddingText(font)}</td>
												<td>{encodingText(font.encoding)}</td>
											</tr>
										{/each}
									</tbody>
								</table>
								{#if attention}
									<p class="setting-note mt-3">
										Text in a font that is not embedded is drawn by each reader with a font of its own, so it can look
										different, or show the wrong characters, from one reader to another.
									</p>
								{/if}
							{/if}
						</Tabs.Content>
						<div class="properties-footer">
							<button type="button" class="button" onclick={close}>Cancel</button>
							<button type="submit" class="button button-primary">OK</button>
						</div>
					</form>
				</Tabs.Root>
			{/if}
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
