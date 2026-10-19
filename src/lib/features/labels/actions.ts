// Page label actions shared by the Page labels dialog and the grid of pages being
// organized, where it is opened (sections 6.3 and 6.7). The dialog numbers the pages
// picked in it; every applied change sends the whole list of rules to Rust as one
// operation, so it is one undo step.

import { refuseRestricted } from '#lib/features/unprotect/actions.ts';
import type { LabelRule } from '#lib/ipc/index.ts';
import { app } from '#lib/stores/app.svelte.ts';
import type { DocTab } from '#lib/stores/doc.svelte.ts';

import { assignLabels, defaultRule, editableRules, sameRules, type Numbering } from './rules.ts';

/** The rules a change starts from: the document's, as `editableRules` reads them. */
export function shownRules(tab: DocTab): LabelRule[] {
	return editableRules(tab.labelRules, tab.pageCount).rules;
}

/** Tells the user why labels can't be changed, if they can't. */
function refuseEdit(tab: DocTab): boolean {
	if (tab.canEditLabels) return false;
	if (app.refuseReadOnly(tab)) return true;
	refuseRestricted(tab, 'This document’s security settings don’t allow changing page labels.');
	return true;
}

/**
 * Label changes run one after another. Each computes its rules from the labels as they are
 * when it runs, so two of them started close together don't overwrite each other.
 */
let queue: Promise<unknown> = Promise.resolve();

function queued<T>(run: () => Promise<T>): Promise<T> {
	const next = queue.then(run, run);
	queue = next.catch(() => {});
	return next;
}

/**
 * Opens the Page labels dialog over the grid of the document's pages: it is part of the
 * page organizing tool, and does nothing while the document is read. The dialog starts
 * with the pages selected in the grid, or else with the page that has the keyboard focus
 * there.
 */
export function showLabels(tab: DocTab) {
	if (!tab.organize || app.labelsRequest || refuseEdit(tab)) return;
	app.labelsRequest = { tabId: tab.id };
}

/**
 * Applies `rules` (or removes all labels, for an empty list). Nothing is sent when the
 * rules are what the document already has, so that is not an edit: a document without
 * labels does not get a "1, 2, 3" tree, and rules another app stored past the last page
 * stay until a real change.
 * Returns true if the document now has `rules`.
 */
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

/**
 * Numbers `pages` (0-based, sorted) as `numbering` says; every other page keeps its label.
 * A document numbered 1, 2, 3 throughout needs no labels, so it is left without any.
 * Returns true if the document now has those labels.
 */
export function assign(tab: DocTab, pages: readonly number[], numbering: Numbering): Promise<boolean> {
	return queued(() => {
		const rules = assignLabels(shownRules(tab), tab.pageCount, pages, numbering);
		return applyRules(tab, sameRules(rules, [defaultRule()]) ? [] : rules);
	});
}
