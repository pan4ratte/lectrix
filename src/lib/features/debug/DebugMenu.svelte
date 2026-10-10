<script lang="ts">
	// The Debug menu, after Help, in development builds or with LECTRIX_DEBUG=1: shows each
	// notification, notice and prompt on demand (rehearse.ts), so they can be looked at and
	// clicked through without an update, a crash or another program.
	import { ChevronRight } from '@lucide/svelte';
	import { Menubar } from 'bits-ui';

	import { toAppError } from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	import {
		clearRehearsals,
		rehearseBanner,
		rehearseError,
		rehearseInfo,
		rehearseRecovery,
		rehearseUpdate,
		rehearseUpdated,
		rehearseUpdateError,
		rehearseUpToDate
	} from './rehearse.ts';

	/** A submenu's first item level with the item that opens it (as in MenuBar). */
	const SUBMENU_OFFSET = -5;
	const hasDoc = $derived(app.active !== null);
	/** The start screen is on screen: no document, and not the Combine view. */
	const onStartScreen = $derived(app.active === null && !app.combineActive);

	/** Runs a rehearsal, saying so when it fails rather than showing nothing. */
	function run(rehearsal: () => Promise<unknown>) {
		return () => void rehearsal().catch((e: unknown) => app.showError(toAppError(e)));
	}
</script>

{#snippet chevron()}
	<ChevronRight size={14} class="ml-auto text-fg-muted" aria-hidden="true" />
{/snippet}

<Menubar.Menu>
	<Menubar.Trigger class="menubar-trigger">Debug</Menubar.Trigger>
	<Menubar.Portal>
		<Menubar.Content class="menu-content" align="start" sideOffset={7}>
			<Menubar.Sub>
				<Menubar.SubTrigger class="menu-item">Update notice{@render chevron()}</Menubar.SubTrigger>
				<Menubar.SubContent class="menu-content" align="start" alignOffset={SUBMENU_OFFSET}>
					<Menubar.Item class="menu-item" onSelect={run(() => rehearseUpdate('checking'))}>Checking for updates</Menubar.Item>
					<Menubar.Item class="menu-item" onSelect={run(() => rehearseUpdate('available'))}>Update available</Menubar.Item>
					<Menubar.Item class="menu-item" onSelect={run(() => rehearseUpdate('downloading'))}>Downloading</Menubar.Item>
					<Menubar.Item class="menu-item" onSelect={run(() => rehearseUpdate('ready'))}>
						Ready to install (Windows)
					</Menubar.Item>
					<Menubar.Item class="menu-item" onSelect={run(() => rehearseUpdate('installed'))}>
						Installed (macOS, Linux)
					</Menubar.Item>
					<Menubar.Separator class="menu-separator" />
					<Menubar.Item class="menu-item" onSelect={run(() => rehearseUpToDate())}>Up to date</Menubar.Item>
					<Menubar.Item class="menu-item" onSelect={() => rehearseUpdateError('check')}>Check failed</Menubar.Item>
					<Menubar.Item class="menu-item" onSelect={() => rehearseUpdateError('download')}>Download failed</Menubar.Item>
				</Menubar.SubContent>
			</Menubar.Sub>
			<Menubar.Sub>
				<Menubar.SubTrigger class="menu-item">After an update{@render chevron()}</Menubar.SubTrigger>
				<Menubar.SubContent class="menu-content" align="start" alignOffset={SUBMENU_OFFSET}>
					<Menubar.Item
						class="menu-item"
						disabled={!onStartScreen}
						onSelect={run(() => rehearseUpdated('start screen'))}
					>
						On the start screen
					</Menubar.Item>
					<Menubar.Item class="menu-item" onSelect={run(() => rehearseUpdated('notification'))}>
						As a notification
					</Menubar.Item>
				</Menubar.SubContent>
			</Menubar.Sub>
			<Menubar.Sub>
				<Menubar.SubTrigger class="menu-item">Notification{@render chevron()}</Menubar.SubTrigger>
				<Menubar.SubContent class="menu-content" align="start" alignOffset={SUBMENU_OFFSET}>
					<Menubar.Item class="menu-item" onSelect={rehearseInfo}>Information</Menubar.Item>
					<Menubar.Item class="menu-item" onSelect={() => rehearseError(false)}>Error</Menubar.Item>
					<Menubar.Item class="menu-item" onSelect={() => rehearseError(true)}>Error with a button</Menubar.Item>
				</Menubar.SubContent>
			</Menubar.Sub>
			<Menubar.Sub>
				<Menubar.SubTrigger class="menu-item" disabled={!hasDoc}>
					File changed on disk{@render chevron()}
				</Menubar.SubTrigger>
				<Menubar.SubContent class="menu-content" align="start" alignOffset={SUBMENU_OFFSET}>
					<Menubar.Item class="menu-item" onSelect={() => rehearseBanner('changedOnDisk')}>Changed</Menubar.Item>
					<Menubar.Item class="menu-item" onSelect={() => rehearseBanner('changedOnDiskDirty')}>
						Changed, with unsaved changes
					</Menubar.Item>
					<Menubar.Item class="menu-item" onSelect={() => rehearseBanner('deletedOnDisk')}>Removed</Menubar.Item>
				</Menubar.SubContent>
			</Menubar.Sub>
			<Menubar.Sub>
				<Menubar.SubTrigger class="menu-item">Crash recovery{@render chevron()}</Menubar.SubTrigger>
				<Menubar.SubContent class="menu-content" align="start" alignOffset={SUBMENU_OFFSET}>
					<Menubar.Item class="menu-item" onSelect={run(() => rehearseRecovery(1))}>One document</Menubar.Item>
					<Menubar.Item class="menu-item" onSelect={run(() => rehearseRecovery(3))}>Several documents</Menubar.Item>
				</Menubar.SubContent>
			</Menubar.Sub>
			<Menubar.Separator class="menu-separator" />
			<Menubar.Item class="menu-item" onSelect={clearRehearsals}>Clear all</Menubar.Item>
		</Menubar.Content>
	</Menubar.Portal>
</Menubar.Menu>
