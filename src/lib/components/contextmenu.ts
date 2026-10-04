// Context menus from the keyboard (accessibility pass, section 8): Shift+F10 and the Menu
// key open the context menu of the focused control, as a right-click there would.

/** True for the keys Windows uses to open a context menu. */
export function isContextMenuKey(event: KeyboardEvent): boolean {
	return event.key === 'ContextMenu' || (event.key === 'F10' && event.shiftKey && !event.ctrlKey && !event.altKey);
}

/**
 * Sends a right-click to the focused control: to the row a list or tree points at
 * (aria-activedescendant) when it keeps focus itself, near its top-left corner, so menus
 * that act on what is under the pointer (a page, a bookmark) act on it.
 */
export function openContextMenu(target: EventTarget | null): boolean {
	if (!(target instanceof HTMLElement)) return false;
	const activeId = target.getAttribute('aria-activedescendant');
	const element = (activeId ? document.getElementById(activeId) : null) ?? target;
	const r = element.getBoundingClientRect();
	const clientX = Math.min(Math.max(r.left + Math.min(r.width / 2, 40), 0), innerWidth - 1);
	const clientY = Math.min(Math.max(r.top + Math.min(r.height / 2, 40), 0), innerHeight - 1);
	element.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, button: 2, clientX, clientY }));
	return true;
}
