<script lang="ts">
	// macOS: the app's menus in the system's menu bar (section 8, menu.rs), arranged as Mac
	// apps arrange theirs (menus.svelte.ts). This window sends its menus whenever they change;
	// Rust shows those of the window in use and passes the item chosen back here, to run.
	import { onMount } from 'svelte';

	import type { CommandName } from '#lib/commands.ts';
	import { onMenuCommand, setMenu } from '#lib/ipc/index.ts';
	import { appMenus, editing, nativeMenus, runMenuItem } from '#lib/menus.svelte.ts';
	import { COMMIT_FIELD_FIRST, isTextInput } from '#lib/shortcuts.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	const menus = $derived(appMenus('macos'));
	const native = $derived(nativeMenus(menus, app.modalOpen));

	let sent = '';
	$effect(() => {
		const json = JSON.stringify(native);
		if (json === sent) return;
		sent = json;
		void setMenu(native).catch(() => {
			sent = '';
		});
	});

	/** A dialog is on screen, one the app's state does not name (a progress dialog). */
	const dialogShown = () => document.querySelector('.dialog-content[data-state="open"]') !== null;

	function chosen(id: string) {
		const field = document.activeElement;
		const typing = isTextInput(field);
		// The menu bar gets ⌘Z and ⇧⌘Z in a text field: they edit its text.
		if (typing && (id === 'undo' || id === 'redo')) {
			document.execCommand(id);
			return;
		}
		if (app.modalOpen || dialogShown()) return;
		// Saving or closing from a text field: the field commits its edit first (as the
		// shortcuts have it).
		if (typing && COMMIT_FIELD_FIRST.has(id as CommandName)) (field as HTMLElement).blur();
		runMenuItem(menus, id);
	}

	onMount(() => {
		const follow = () => (editing.text = isTextInput(document.activeElement));
		// After a field loses the focus, the next has it only once the event is over.
		const left = () => queueMicrotask(follow);
		document.addEventListener('focusin', follow);
		document.addEventListener('focusout', left);
		const unlisten = onMenuCommand(chosen);
		return () => {
			document.removeEventListener('focusin', follow);
			document.removeEventListener('focusout', left);
			void unlisten.then((f) => f());
		};
	});
</script>
