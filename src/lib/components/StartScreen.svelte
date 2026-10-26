<script lang="ts">
	// Shown when no document is open: the tools as cards (open a file, combine files...), or a
	// recent file to pick; in a wide window the tools are a column at the left, three cards
	// to a row, beside the recent files. Recent files show as
	// a table of their names, when each was last opened and its size, or as a grid of their
	// first pages with the name, date and size under each (a switch beside the heading,
	// remembered in app data; switching brings the files in with a short animation). A click
	// on a row or card opens the file; the folder is in the name's tooltip. A star pins a file
	// to the top of the list, and a right-click offers opening it, pinning it, showing it in
	// its folder and removing it. After an update, a block beside the app's name links to
	// What's new. Also shown over open documents, from the title bar's icon and name.
	import { ArrowUpRight, FolderOpen, LayoutGrid, Layers, List, Lock, LockOpen, Shrink, Sparkles, Star, X } from '@lucide/svelte';
	import { ContextMenu } from 'bits-ui';
	import type { Component } from 'svelte';

	import fileIconUrl from '#lib/assets/lectrix-file-icon-small.svg';
	import iconUrl from '#lib/assets/lectrix-icon.svg';
	import { APP_NAME } from '#lib/config.ts';
	import { openWhatsNew } from '#lib/features/changelog/announce.ts';
	import { toolMark } from '#lib/features/annotations/toolMark.ts';
	import { compressFile } from '#lib/features/compress/actions.ts';
	import { organizeFile } from '#lib/features/organize/actions.ts';
	import { protectFile } from '#lib/features/protect/actions.ts';
	import { unlockFile } from '#lib/features/unprotect/actions.ts';
	import { fileSize, fullDate, openedDate } from '#lib/format.ts';
	import {
		recentPreviewUrl,
		removeRecent,
		setRecentPinned,
		showRecentInFolder,
		toAppError,
		type RecentFile
	} from '#lib/ipc/index.ts';
	import type { CommandName } from '#lib/commands.ts';
	import { PLATFORM } from '#lib/platform.ts';
	import { app } from '#lib/stores/app.svelte.ts';


	interface Tool {
		id: string;
		/** As a card names it: without the menus' "…". */
		label: string;
		/** One line on what it does, under the name. */
		description: string;
		icon: Component<{ size?: number; strokeWidth?: number; 'aria-hidden'?: boolean | 'true' }>;
		/** The command whose shortcut the card shows. */
		command?: CommandName;
		run: () => void;
	}

	/** The tools, as cards in a grid of three columns; more will come. */
	const TOOLS: readonly Tool[] = [
		{ id: 'open', label: 'Open', description: 'Browse for a PDF', icon: FolderOpen, command: 'open', run: () => void app.open() },
		{ id: 'combine', label: 'Combine files', description: 'Merge PDFs into one', icon: Layers, run: () => app.openCombine() },
		{ id: 'organize', label: 'Organize pages', description: 'Reorder, rotate or delete', icon: LayoutGrid, run: () => void organizeFile() },
		{ id: 'unlock', label: 'Unlock', description: 'Remove a password', icon: LockOpen, command: 'unlockFile', run: () => void unlockFile() },
		{ id: 'protect', label: 'Protect', description: 'Add a password', icon: Lock, command: 'protectFile', run: () => void protectFile() },
		{ id: 'compress', label: 'Compress', description: 'Reduce file size', icon: Shrink, command: 'compressFile', run: () => void compressFile() }
	];

	/** A press's ripple, from where it was pressed in the card (CSS pixels of the card, as
	 * any zoom of the page leaves them); none with reduced motion. */
	function ripple(card: HTMLElement, x: number, y: number) {
		if (matchMedia('(prefers-reduced-motion: reduce)').matches) return;
		const layer = card.querySelector('.start-ripples');
		if (!layer) return;
		const dot = document.createElement('span');
		dot.className = 'start-ripple';
		dot.style.left = `${x}px`;
		dot.style.top = `${y}px`;
		const done = () => dot.remove();
		dot.addEventListener('animationend', done, { once: true });
		dot.addEventListener('animationcancel', done, { once: true });
		// In case the animation never runs (the card hidden at once): gone after its time.
		setTimeout(done, 1000);
		layer.append(dot);
	}

	function pressed(event: PointerEvent) {
		if (event.button !== 0) return;
		const card = event.currentTarget as HTMLElement;
		const box = card.getBoundingClientRect();
		const scale = box.width ? card.offsetWidth / box.width : 1;
		ripple(card, (event.clientX - box.left) * scale, (event.clientY - box.top) * scale);
	}

	function keyed(event: KeyboardEvent) {
		if ((event.key !== 'Enter' && event.key !== ' ') || event.repeat) return;
		const card = event.currentTarget as HTMLElement;
		ripple(card, card.offsetWidth / 2, card.offsetHeight / 2);
	}

	/** The table or the grid was picked since the start screen showed: the files come in
	 * animated (not the first time they show). */
	let switched = $state(false);
	/** How many files come in one after another; the rest come with the last of them. */
	const STAGGERED = 8;

	function showAs(grid: boolean) {
		if (app.recentFilesGrid === grid) return;
		switched = true;
		app.recentFilesGrid = grid;
	}

	/** The widest a preview shows, CSS pixels; rendered for the screen's pixel ratio. */
	const PREVIEW_WIDTH = 200;

	/** Previews that loaded, and ones that could not be made (shown as the file icon). */
	let loaded = $state<Record<string, true>>({});
	let failed = $state<Record<string, true>>({});

	async function remove(index: number) {
		await removeRecent(index).catch(() => {});
		await app.refreshRecent();
	}

	/** Pinned files stay at the top of the list, in the order they were pinned. */
	async function pin(file: RecentFile, pinned: boolean) {
		await setRecentPinned(file.index, pinned).catch(() => {});
		await app.refreshRecent();
	}

	async function showInFolder(file: RecentFile) {
		try {
			await showRecentInFolder(file.index);
		} catch (e) {
			app.showError(toAppError(e));
		}
	}

	/** As each system's own file manager words it. */
	const SHOW_IN_FOLDER = PLATFORM === 'macos' ? 'Show in Finder' : PLATFORM === 'linux' ? 'Show in folder' : 'Open file location';

	function previewUrl(file: RecentFile): string {
		const ratio = typeof devicePixelRatio === 'number' ? devicePixelRatio : 1;
		return recentPreviewUrl(file.index, file.openedAt, Math.min(1024, PREVIEW_WIDTH * ratio));
	}

	const folderTip = (file: RecentFile) => (file.exists ? file.folder : `Not found: ${file.folder}`);
	const sizeText = (file: RecentFile) => (file.size === null ? 'Not found' : fileSize(file.size));
</script>

{#snippet pinButton(file: RecentFile, className: string, size: number)}
	<button
		type="button"
		class="icon-button recent-pin {className}"
		class:recent-pinned={file.pinned}
		aria-label="Pin {file.name}"
		aria-pressed={file.pinned}
		data-tip={file.pinned ? 'Unpin' : 'Pin'}
		onclick={() => void pin(file, !file.pinned)}
	>
		<Star {size} fill={file.pinned ? 'currentColor' : 'none'} aria-hidden="true" />
	</button>
{/snippet}

{#snippet fileMenu(file: RecentFile)}
	<ContextMenu.Portal>
		<ContextMenu.Content class="menu-content">
			<ContextMenu.Item class="menu-item" onSelect={() => void app.openRecent(file.index)}>Open</ContextMenu.Item>
			<ContextMenu.Item class="menu-item" onSelect={() => void pin(file, !file.pinned)}>
				{file.pinned ? 'Unpin' : 'Pin to the top of the list'}
			</ContextMenu.Item>
			<ContextMenu.Item class="menu-item" onSelect={() => void showInFolder(file)}>{SHOW_IN_FOLDER}</ContextMenu.Item>
			<ContextMenu.Separator class="menu-separator" />
			<ContextMenu.Item class="menu-item" onSelect={() => void remove(file.index)}>Remove from the list</ContextMenu.Item>
		</ContextMenu.Content>
	</ContextMenu.Portal>
{/snippet}

{#snippet removeButton(file: RecentFile, className: string, size: number)}
	<button
		type="button"
		class="icon-button recent-remove {className}"
		aria-label="Remove {file.name} from the list"
		data-tip="Remove from list"
		onclick={() => void remove(file.index)}
	>
		<X {size} aria-hidden="true" />
	</button>
{/snippet}

<div class="start-screen flex h-full flex-col items-center overflow-auto">
	<div class="start-content" class:start-split={app.recent.length > 0}>
		<div class="flex flex-wrap items-center gap-[16px]">
			<!-- Decorative: the heading names the app. The icon's rounded square is 46 px; the
			     image has a margin round it. -->
			<img src={iconUrl} alt="" width="57" height="57" class="start-icon" draggable="false" />
			<div class="flex flex-col gap-[3px]">
				<h1 class="start-title">{APP_NAME}</h1>
				<p class="text-[14px] text-fg-muted">Open a PDF, or drop files anywhere in this window.</p>
			</div>
			{#if app.updatedTo !== null}
				<div class="start-update" role="status">
					<span class="shrink-0 text-accent"><Sparkles size={18} aria-hidden="true" /></span>
					<div class="min-w-0 flex-1">
						<p class="font-medium">{app.updatedTo ? `Updated to ${app.updatedTo}` : 'Updated'}</p>
						<button
							type="button"
							class="notice-link text-sm"
							onclick={() => {
								app.updatedTo = null;
								openWhatsNew();
							}}
						>
							See what’s new
						</button>
					</div>
					<button
						type="button"
						class="icon-button size-6 shrink-0"
						aria-label="Dismiss"
						onclick={() => (app.updatedTo = null)}
					>
						<X size={14} aria-hidden="true" />
					</button>
				</div>
			{/if}
		</div>
		<div class="start-body mt-[26px]">
			<div class="start-tools">
				{#each TOOLS as tool (tool.id)}
					{@const shortcut = tool.command ? app.shortcut(tool.command) : ''}
					<button
						type="button"
						class="start-tool"
						data-tool={tool.id}
						aria-labelledby="start-tool-{tool.id}"
						aria-describedby={shortcut ? `start-tool-${tool.id}-about start-tool-${tool.id}-keys` : `start-tool-${tool.id}-about`}
						onpointerdown={pressed}
						onkeydown={keyed}
						onclick={tool.run}
					>
						<span class="start-ripples" aria-hidden="true"></span>
						<span class="start-tool-top">
							<span class="start-tool-icon"><tool.icon size={22} aria-hidden="true" /></span>
							{#if shortcut}
								<span id="start-tool-{tool.id}-keys" class="start-tool-shortcut">{shortcut}</span>
							{:else}
								<span class="start-tool-arrow" aria-hidden="true"><ArrowUpRight size={18} /></span>
							{/if}
						</span>
						<span class="start-tool-text">
							<span id="start-tool-{tool.id}" class="start-tool-label truncate">{tool.label}</span>
							<span id="start-tool-{tool.id}-about" class="start-tool-description">{tool.description}</span>
						</span>
					</button>
				{/each}
			</div>

			{#if app.recent.length}
				<div class="start-recent">
					<div class="mb-[10px] flex items-center justify-between gap-4">
						<h2 id="recent-heading" class="recent-heading">Recent files</h2>
						<!-- The mark of the view shown glides to the one picked, as the active tool's does. -->
						<div class="tool-group flex gap-[4px]" role="group" aria-label="Show recent files as" {@attach toolMark}>
							<span class="tool-mark" aria-hidden="true" hidden></span>
							<button
								type="button"
								class="icon-button tool-button"
								aria-label="List"
								data-tip="List"
								aria-pressed={!app.recentFilesGrid}
								onclick={() => showAs(false)}
							>
								<List size={18} aria-hidden="true" />
							</button>
							<button
								type="button"
								class="icon-button tool-button"
								aria-label="Grid"
								data-tip="Grid"
								aria-pressed={app.recentFilesGrid}
								onclick={() => showAs(true)}
							>
								<LayoutGrid size={18} aria-hidden="true" />
							</button>
						</div>
					</div>

					{#if app.recentFilesGrid}
						<ul class="recent-grid" class:recent-enter={switched} aria-labelledby="recent-heading">
							{#each app.recent as file, i (file.index)}
								{@const url = previewUrl(file)}
								<ContextMenu.Root>
									<ContextMenu.Trigger>
										{#snippet child({ props })}
											<li {...props} class="recent-card" class:recent-missing={!file.exists} style:--recent-i={Math.min(i, STAGGERED)}>
												<button type="button" class="recent-open" data-tip={folderTip(file)} onclick={() => void app.openRecent(file.index)}>
													<span class="recent-preview">
														{#if file.exists && !failed[url]}
															<img
																src={url}
																alt=""
																class="recent-page"
																class:recent-page-loaded={loaded[url]}
																draggable="false"
																decoding="async"
																onload={() => (loaded[url] = true)}
																onerror={() => (failed[url] = true)}
															/>
														{/if}
														{#if !file.exists || failed[url]}
															<!-- Decorative: the name says what it is. -->
															<img src={fileIconUrl} alt="" class="h-12 w-auto" draggable="false" />
														{/if}
													</span>
													<span class="recent-name">{file.name}</span>
													<span class="recent-details">
														<span data-tip={fullDate(file.openedAt)}>{openedDate(file.openedAt)}</span>
														<span aria-hidden="true">·</span>
														<span class="tabular-nums">{sizeText(file)}</span>
													</span>
												</button>
												{@render pinButton(file, 'recent-card-button recent-card-pin size-7', 14)}
												{@render removeButton(file, 'recent-card-button recent-card-remove size-7', 14)}
											</li>
										{/snippet}
									</ContextMenu.Trigger>
									{@render fileMenu(file)}
								</ContextMenu.Root>
							{/each}
						</ul>
					{:else}
						<table class="recent-table" class:recent-enter={switched} aria-labelledby="recent-heading">
							<colgroup>
								<col />
								<col class="recent-col-opened" />
								<col class="recent-col-size" />
								<col class="recent-col-actions" />
							</colgroup>
							<thead>
								<tr>
									<th scope="col">Name</th>
									<th scope="col">Opened</th>
									<th scope="col" class="text-right">Size</th>
									<th scope="col"><span class="sr-only">Actions</span></th>
								</tr>
							</thead>
							<tbody>
								{#each app.recent as file, i (file.index)}
									<ContextMenu.Root>
										<ContextMenu.Trigger>
											{#snippet child({ props })}
												<tr {...props} class="recent-row" class:recent-missing={!file.exists} style:--recent-i={Math.min(i, STAGGERED)}>
													<td>
														<button
															type="button"
															class="recent-open recent-open-row"
															data-tip={folderTip(file)}
															onclick={() => void app.openRecent(file.index)}
														>
															<!-- Decorative: the name says what it is. -->
															<img src={fileIconUrl} alt="" height="20" class="h-5 w-auto shrink-0" draggable="false" />
															<span class="truncate">{file.name}</span>
														</button>
													</td>
													<td class="truncate" data-tip={fullDate(file.openedAt)}>{openedDate(file.openedAt)}</td>
													<td class="text-right tabular-nums">{sizeText(file)}</td>
													<td>
														<div class="recent-actions">
															{@render pinButton(file, '', 18)}
															{@render removeButton(file, '', 18)}
														</div>
													</td>
												</tr>
											{/snippet}
										</ContextMenu.Trigger>
										{@render fileMenu(file)}
									</ContextMenu.Root>
								{/each}
							</tbody>
						</table>
					{/if}
				</div>
			{/if}
		</div>
	</div>
</div>
