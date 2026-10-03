// Page label actions shared by the panel, the menus and the thumbnails (section 6.3).
// The panel edits a list of rules; every applied change sends the whole list to Rust as
// one operation, so it is one undo step.

import type { LabelRule } from '#lib/ipc/index.ts';
import { app } from '#lib/stores/app.svelte.ts';
import type { DocTab } from '#lib/stores/doc.svelte.ts';

import { editableRules, romanThenArabic, sameRules, withRuleAt } from './rules.ts';

/** The rules the panel shows for the document. */
export function shownRules(tab: DocTab): LabelRule[] {
	return editableRules(tab.labelRules, tab.pageCount).rules;
}

/** Tells the user why labels can't be changed, if they can't. */
function refuseEdit(tab: DocTab): boolean {
	if (tab.canEditLabels) return false;
	app.notify({
		kind: 'error',
		message: 'This document’s security settings don’t allow changing page labels.',
		suggestion: 'Ask the document’s author for an unrestricted copy.'
	});
	return true;
}

/**
 * Label actions run one after another. Each computes its rules from the labels as they are
 * when it runs, so a field applied on blur and the click that caused the blur (Delete
 * range, a preset) don't overwrite each other.
 */
let queue: Promise<unknown> = Promise.resolve();

function queued<T>(run: () => Promise<T>): Promise<T> {
	const next = queue.then(run, run);
	queue = next.catch(() => {});
	return next;
}

export function showLabels() {
	app.sidebarOpen = true;
	app.sidebarPanel = 'labels';
}

/**
 * Applies `rules` (or removes all labels, for an empty list). Nothing is sent when the
 * rules are what the panel already shows, so leaving a field unchanged is not an edit: a
 * document without labels does not get a "1, 2, 3" tree, and rules another app stored
 * past the last page stay until a real change.
 * Returns true if the panel now shows `rules`.
 */
export function setRules(tab: DocTab, rules: LabelRule[]): Promise<boolean> {
	return queued(() => applyRules(tab, rules));
}

async function applyRules(tab: DocTab, rules: LabelRule[]): Promise<boolean> {
	const unchanged = rules.length === 0 ? tab.labels === null : sameRules(rules, shownRules(tab));
	if (unchanged) {
		tab.previewLabels = null;
		return true;
	}
	if (refuseEdit(tab)) {
		tab.previewLabels = null;
		return false;
	}
	const change = await app.apply(tab, { kind: 'setPageLabels', rules });
	tab.previewLabels = null;
	return change !== null;
}

/** Starts a new label range (1, 2, 3 from 1) at `page` and selects it in the panel. */
export function startRangeAt(tab: DocTab, page: number) {
	showLabels();
	return queued(async () => {
		const rules = shownRules(tab);
		if (rules.some((r) => r.startPage === page) || (await applyRules(tab, withRuleAt(rules, page)))) {
			tab.selectedLabelRule = page;
		}
	});
}

/** Deletes the rule starting at `page` (never the first one); its pages join the range before. */
export function deleteRule(tab: DocTab, page: number) {
	return queued(async () => {
		const rules = shownRules(tab);
		const index = rules.findIndex((r) => r.startPage === page);
		if (index <= 0) return;
		if (await applyRules(tab, rules.filter((r) => r.startPage !== page))) {
			tab.selectedLabelRule = rules[index - 1]!.startPage;
		}
	});
}

/** "Roman front matter, then arabic from this page": replaces every rule. */
export function romanFrontMatter(tab: DocTab, page: number) {
	if (page <= 0) return Promise.resolve();
	showLabels();
	return queued(async () => {
		if (await applyRules(tab, romanThenArabic(page))) tab.selectedLabelRule = page;
	});
}

/** "Remove all labels." */
export function removeLabels(tab: DocTab) {
	return queued(async () => {
		if (await applyRules(tab, [])) tab.selectedLabelRule = 0;
	});
}
