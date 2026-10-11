<script lang="ts">
	// The app's menus, in the custom title bar.
	import { ChevronRight } from '@lucide/svelte';
	import { Menubar } from 'bits-ui';

	import { commands, setViewMode, type CommandName } from '#lib/commands.ts';
	import { APP_NAME } from '#lib/config.ts';
	import DebugMenu from '#lib/features/debug/DebugMenu.svelte';
	import { update } from '#lib/features/update/update.svelte.ts';
	import { VIEW_MODES } from '#lib/features/viewer/layout.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	const tab = $derived(app.active);
	const hasDoc = $derived(tab !== null);

	/** A submenu's first item level with the item that opens it: up by the menu's padding
	 * (4 px) and edge (1 px). */
	const SUBMENU_OFFSET = -5;
</script>

<!-- A command's shortcut, as Settings has it. -->
{#snippet keys(command: CommandName)}
	{@const text = app.shortcut(command)}
	{#if text}<span class="menu-shortcut">{text}</span>{/if}
{/snippet}

{#snippet chevron()}
	<ChevronRight size={14} class="ml-auto text-fg-muted" aria-hidden="true" />
{/snippet}

<!-- The menus open from the title bar's bottom edge, as in VS Code: the 22 px triggers sit about 7 px
     above it in the 35 px bar. -->
<Menubar.Root class="flex items-center" aria-label="Main menu">
	<Menubar.Menu>
		<Menubar.Trigger class="menubar-trigger">File</Menubar.Trigger>
		<Menubar.Portal>
			<Menubar.Content class="menu-content" align="start" sideOffset={7}>
				<Menubar.Item class="menu-item" onSelect={commands.open}>
					Open…{@render keys('open')}
				</Menubar.Item>
				<Menubar.Sub>
					<Menubar.SubTrigger class="menu-item" disabled={app.recent.length === 0}>
						Open recent{@render chevron()}
					</Menubar.SubTrigger>
					<Menubar.SubContent class="menu-content max-w-[480px]" align="start" alignOffset={SUBMENU_OFFSET}>
						{#each app.recent as file (file.index)}
							<Menubar.Item class="menu-item" onSelect={() => void app.openRecent(file.index)}>
								<span class="truncate" class:text-fg-muted={!file.exists} title="{file.folder}\{file.name}">
									{file.name}
								</span>
							</Menubar.Item>
						{/each}
					</Menubar.SubContent>
				</Menubar.Sub>
				<Menubar.Item class="menu-item" onSelect={commands.combineFiles}>Combine files…{@render keys('combineFiles')}</Menubar.Item>
				<Menubar.Separator class="menu-separator" />
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.save}>
					Save{@render keys('save')}
				</Menubar.Item>
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.saveAs}>
					Save as…{@render keys('saveAs')}
				</Menubar.Item>
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.saveAsOptimized}>
					Save as (optimized)…{@render keys('saveAsOptimized')}
				</Menubar.Item>
				<Menubar.Separator class="menu-separator" />
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.reload}>
					Reload from disk{@render keys('reload')}
				</Menubar.Item>
				<Menubar.Item class="menu-item" disabled={!hasDoc && !app.combineActive} onSelect={commands.closeTab}>
					Close{@render keys('closeTab')}
				</Menubar.Item>
				<Menubar.Separator class="menu-separator" />
				<Menubar.Item class="menu-item" onSelect={commands.settings}>
					Settings…{@render keys('settings')}
				</Menubar.Item>
				<Menubar.Separator class="menu-separator" />
				<Menubar.Item class="menu-item" onSelect={commands.exit}>Exit{@render keys('exit')}</Menubar.Item>
			</Menubar.Content>
		</Menubar.Portal>
	</Menubar.Menu>

	<Menubar.Menu>
		<Menubar.Trigger class="menubar-trigger">Edit</Menubar.Trigger>
		<Menubar.Portal>
			<Menubar.Content class="menu-content" align="start" sideOffset={7}>
				{#if app.combineActive}
					<Menubar.Item class="menu-item" disabled={!app.combine?.canUndo} onSelect={commands.undo}>
						Undo{@render keys('undo')}
					</Menubar.Item>
					<Menubar.Item class="menu-item" disabled={!app.combine?.canRedo} onSelect={commands.redo}>
						Redo{@render keys('redo')}
					</Menubar.Item>
				{:else}
					<Menubar.Item class="menu-item" disabled={!tab?.state.undoName} onSelect={commands.undo}>
						{tab?.state.undoName ? `Undo ${tab.state.undoName.toLowerCase()}` : 'Undo'}{@render keys('undo')}
						
					</Menubar.Item>
					<Menubar.Item class="menu-item" disabled={!tab?.state.redoName} onSelect={commands.redo}>
						{tab?.state.redoName ? `Redo ${tab.state.redoName.toLowerCase()}` : 'Redo'}{@render keys('redo')}
						
					</Menubar.Item>
				{/if}
				<Menubar.Separator class="menu-separator" />
				<Menubar.Item class="menu-item" disabled={!tab?.selection} onSelect={commands.copy}>
					Copy{@render keys('copy')}
				</Menubar.Item>
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.find}>
					Find…{@render keys('find')}
				</Menubar.Item>
			</Menubar.Content>
		</Menubar.Portal>
	</Menubar.Menu>

	<Menubar.Menu>
		<Menubar.Trigger class="menubar-trigger">View</Menubar.Trigger>
		<Menubar.Portal>
			<Menubar.Content class="menu-content" align="start" sideOffset={7}>
				<Menubar.CheckboxItem
					class="menu-item"
					disabled={!hasDoc || app.panels.left.length === 0}
					checked={app.leftOpen}
					onCheckedChange={commands.toggleLeftPane}
				>
					{#snippet children({ checked })}
						<span class="w-4" aria-hidden="true">{checked ? '✓' : ''}</span>Left pane{@render keys('toggleLeftPane')}
					{/snippet}
				</Menubar.CheckboxItem>
				<Menubar.CheckboxItem
					class="menu-item"
					disabled={!hasDoc || app.panels.right.length === 0}
					checked={app.rightOpen}
					onCheckedChange={commands.toggleRightPane}
				>
					{#snippet children({ checked })}
						<span class="w-4" aria-hidden="true">{checked ? '✓' : ''}</span>Right pane{@render keys('toggleRightPane')}
					{/snippet}
				</Menubar.CheckboxItem>
				<Menubar.Separator class="menu-separator" />
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.showPages}>Pages{@render keys('showPages')}</Menubar.Item>
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.showBookmarks}>
					Bookmarks{@render keys('showBookmarks')}
				</Menubar.Item>
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.showLabels}>
					Page labels{@render keys('showLabels')}
				</Menubar.Item>
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.showAnnotations}>
					Annotations{@render keys('showAnnotations')}
				</Menubar.Item>
				<Menubar.Separator class="menu-separator" />
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.zoomIn}>
					Zoom in{@render keys('zoomIn')}
				</Menubar.Item>
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.zoomOut}>
					Zoom out{@render keys('zoomOut')}
				</Menubar.Item>
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.fitWidth}>
					Fit width{@render keys('fitWidth')}
				</Menubar.Item>
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.fitPage}>
					Fit page{@render keys('fitPage')}
				</Menubar.Item>
				<Menubar.Separator class="menu-separator" />
				<Menubar.Sub>
					<Menubar.SubTrigger class="menu-item" disabled={!hasDoc}>Page display{@render chevron()}</Menubar.SubTrigger>
					<Menubar.SubContent class="menu-content" align="start" alignOffset={SUBMENU_OFFSET}>
						<Menubar.RadioGroup
							value={tab?.mode ?? ''}
							onValueChange={(v) => {
								const mode = VIEW_MODES.find((m) => m.id === v);
								if (mode) setViewMode(mode.id);
							}}
						>
							{#each VIEW_MODES as m (m.id)}
								<Menubar.RadioItem class="menu-item" value={m.id}>
									{#snippet children({ checked })}
										<span class="w-4" aria-hidden="true">{checked ? '✓' : ''}</span>{m.label}
									{/snippet}
								</Menubar.RadioItem>
							{/each}
						</Menubar.RadioGroup>
						<Menubar.Separator class="menu-separator" />
						<Menubar.CheckboxItem
							class="menu-item"
							disabled={tab?.columns !== 2}
							checked={tab?.cover ?? false}
							onCheckedChange={commands.toggleCoverPage}
						>
							{#snippet children({ checked })}
								<span class="w-4" aria-hidden="true">{checked ? '✓' : ''}</span>Cover page alone{@render keys('toggleCoverPage')}
							{/snippet}
						</Menubar.CheckboxItem>
					</Menubar.SubContent>
				</Menubar.Sub>
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.rotateViewClockwise}>
					Rotate view clockwise{@render keys('rotateViewClockwise')}
				</Menubar.Item>
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.rotateViewCounterClockwise}>
					Rotate view counter-clockwise{@render keys('rotateViewCounterClockwise')}
				</Menubar.Item>
				<Menubar.Separator class="menu-separator" />
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.goToPage}>
					Go to page…{@render keys('goToPage')}
				</Menubar.Item>
				<Menubar.Item class="menu-item" disabled={!tab?.history.canGoBack} onSelect={commands.back}>
					Back{@render keys('back')}
				</Menubar.Item>
				<Menubar.Item class="menu-item" disabled={!tab?.history.canGoForward} onSelect={commands.forward}>
					Forward{@render keys('forward')}
				</Menubar.Item>
			</Menubar.Content>
		</Menubar.Portal>
	</Menubar.Menu>

	<Menubar.Menu>
		<Menubar.Trigger class="menubar-trigger">Document</Menubar.Trigger>
		<Menubar.Portal>
			<Menubar.Content class="menu-content" align="start" sideOffset={7}>
				<Menubar.Item
					class="menu-item"
					disabled={!hasDoc || !tab?.flags.canAssemble}
					onSelect={commands.rotatePages}
				>
					Rotate pages…{@render keys('rotatePages')}
				</Menubar.Item>
				<Menubar.Item
					class="menu-item"
					disabled={!hasDoc || !tab?.flags.canAssemble}
					onSelect={commands.insertPages}
				>
					Insert pages from file…{@render keys('insertPages')}
				</Menubar.Item>
				<Menubar.Separator class="menu-separator" />
				<Menubar.Item class="menu-item" disabled={!tab?.canEditBookmarks} onSelect={commands.addBookmark}>
					Add bookmark{@render keys('addBookmark')}
				</Menubar.Item>
				<Menubar.Separator class="menu-separator" />
				<Menubar.Item class="menu-item" disabled={!tab?.canEditLabels} onSelect={commands.startLabelRange}>
					New label range from this page{@render keys('startLabelRange')}
				</Menubar.Item>
				<Menubar.Separator class="menu-separator" />
				<Menubar.Item
					class="menu-item"
					disabled={!tab?.flags.canAnnotate || !tab.allAnnotations.length}
					onSelect={commands.repairAnnotations}
				>
					Repair annotations…{@render keys('repairAnnotations')}
				</Menubar.Item>
			</Menubar.Content>
		</Menubar.Portal>
	</Menubar.Menu>

	<Menubar.Menu>
		<Menubar.Trigger class="menubar-trigger">Help</Menubar.Trigger>
		<Menubar.Portal>
			<Menubar.Content class="menu-content" align="start" sideOffset={7}>
				<Menubar.Item class="menu-item" onSelect={commands.whatsNew}>What’s new{@render keys('whatsNew')}</Menubar.Item>
				<Menubar.Item class="menu-item" disabled={update.stage.kind === 'checking'} onSelect={commands.checkForUpdates}>
					Check for updates…{@render keys('checkForUpdates')}
				</Menubar.Item>
				<Menubar.Separator class="menu-separator" />
				<Menubar.Item class="menu-item" onSelect={commands.about}>About {APP_NAME}{@render keys('about')}</Menubar.Item>
			</Menubar.Content>
		</Menubar.Portal>
	</Menubar.Menu>
	<!-- Development builds, or LECTRIX_DEBUG=1. -->
	{#if app.startup?.debugMenu}
		<DebugMenu />
	{/if}
</Menubar.Root>
