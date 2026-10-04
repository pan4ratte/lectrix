// Keyboard and accessible-name checks for the accessibility pass (AGENTS.md section 8):
// what Tab reaches, whether focus is visible there, and what assistive
// technology would call each control.

/** Describes the focused element: tag, role, Chromium's accessible name, focus styles. */
export async function focused(browser) {
	const info = await browser.execute(() => {
		const el = document.activeElement;
		if (!el || el === document.body) return null;
		const transparent = (color) => /rgba\([^)]*,\s*0\)$/.test(color) || color === 'transparent';
		const ring = (node) => {
			const style = getComputedStyle(node);
			const outline =
				style.outlineStyle !== 'none' && parseFloat(style.outlineWidth) > 0 && !transparent(style.outlineColor);
			// Tailwind gives every element transparent, zero-size shadows: those show nothing.
			const shadow =
				style.boxShadow !== 'none' &&
				style.boxShadow.split(/,(?![^(]*\))/).some((s) => {
					const color = /rgba?\([^)]*\)/.exec(s)?.[0] ?? '';
					if (transparent(color)) return false;
					return (s.replace(color, '').match(/-?[\d.]+px/g) ?? []).some((n) => parseFloat(n) !== 0);
				});
			return outline || shadow;
		};
		// A list or tree that keeps focus itself shows it on its active row.
		const activeId = el.getAttribute('aria-activedescendant');
		const active = activeId ? document.getElementById(activeId) : null;
		const visibleFocus = ring(el) || (active !== null && ring(active));
		const r = el.getBoundingClientRect();
		return {
			tag: el.tagName.toLowerCase(),
			role: el.getAttribute('role') ?? '',
			cls: (el.getAttribute('class') ?? '').slice(0, 60),
			visibleFocus,
			onScreen: r.width > 0 && r.height > 0 && r.bottom > 0 && r.right > 0 && r.top < innerHeight && r.left < innerWidth
		};
	});
	if (!info) return null;
	const element = await browser.getActiveElement();
	const id = element['element-6066-11e4-a52e-4f735466cecf'] ?? element.ELEMENT;
	info.name = (await browser.getElementComputedLabel(id).catch(() => '')).trim();
	info.computedRole = await browser.getElementComputedRole(id).catch(() => '');
	return info;
}

/**
 * Presses Tab (or Shift+Tab) until focus comes back to where it started or `max` presses,
 * and returns every stop in order.
 */
export async function tabWalk(browser, { max = 120, back = false } = {}) {
	const stops = [];
	const key = back ? ['Shift', 'Tab'] : ['Tab'];
	let first = null;
	for (let i = 0; i < max; i++) {
		await browser.keys(key);
		const stop = await focused(browser);
		const signature = stop ? `${stop.tag}|${stop.role}|${stop.name}|${stop.cls}` : 'none';
		if (first === null) first = signature;
		else if (signature === first) break;
		stops.push(stop ?? { tag: 'none', name: '', visibleFocus: false, onScreen: false });
	}
	return stops;
}

/** Interactive elements on screen that have no accessible name (Chromium's computation). */
export async function unnamedControls(browser) {
	const elements = await browser.$$(
		'button, a[href], input:not([type="hidden"]), select, textarea, [role="button"], [role="tab"], [role="menuitem"], [role="treeitem"], [role="option"], [role="checkbox"], [role="radio"], [role="slider"], [role="switch"], [tabindex]:not([tabindex="-1"])'
	);
	const unnamed = [];
	for (const el of elements) {
		if (!(await el.isDisplayed().catch(() => false))) continue;
		const name = (await el.getComputedLabel().catch(() => '')).trim();
		if (!name) {
			unnamed.push(
				await el.execute
					? await browser.execute((e) => e.outerHTML.slice(0, 160), el)
					: '(element)'
			);
		}
	}
	return unnamed;
}
