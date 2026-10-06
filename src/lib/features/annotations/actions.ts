// Creating, changing, deleting and repairing annotations (sections 5.3 and 6.5). Every
// change is one operation in Rust, so it is one undo step.

import {
	repairAnnotations as repairCommand,
	scanAnnotationsForRepair,
	toAppError,
	type AnnotationBody,
	type AnnotationEditInput,
	type NewAnnotationInput
} from '#lib/ipc/index.ts';
import { app } from '#lib/stores/app.svelte.ts';
import type { DocTab } from '#lib/stores/doc.svelte.ts';

import { ordered } from '#lib/features/viewer/selection.ts';

import { selectionRanges } from './geometry.ts';
import { tools } from './state.svelte.ts';
import { PROBLEM_SUMMARY, type DrawTool, type MarkupKind } from './tools.ts';

/** Tells the user why nothing happens in a document that forbids annotating. */
export function refuseIfLocked(tab: DocTab): boolean {
	if (tab.flags.canAnnotate) return false;
	app.notify({
		kind: 'error',
		message: 'This document’s security settings don’t allow adding or changing annotations.',
		suggestion: 'Ask the document’s author for a copy that allows annotating.'
	});
	return true;
}

/** Creates annotations with `tool`'s current style (one undo step); with `select`, selects
 * the first. */
export async function create(
	tab: DocTab,
	tool: DrawTool,
	items: { page: number; body: AnnotationBody }[],
	select = true
) {
	if (!items.length || refuseIfLocked(tab)) return null;
	const style = tools.style(tool);
	const annotations: NewAnnotationInput[] = items.map(({ page, body }) => ({
		page,
		color: style.color,
		opacity: style.opacity,
		body
	}));
	const change = await app.apply(tab, { kind: 'addAnnotation', annotations });
	if (select && change?.created != null) tab.selectAnnotation(items[0]!.page, change.created);
	return change;
}

/** Whether new text markup opens its comment (Settings); otherwise it isn't selected. */
export function markupOpensComment(): boolean {
	return app.settings?.openCommentAfterMarkup === true;
}

/**
 * Creates text markup (one undo step). It is not selected, so marking text up leaves
 * nothing in the way of reading on; with `openComment` (by default the setting), it is
 * selected and the inspector opens with the cursor in its comment.
 */
export async function createMarkup(
	tab: DocTab,
	kind: MarkupKind,
	items: { page: number; body: AnnotationBody }[],
	openComment = markupOpensComment()
) {
	const change = await create(tab, kind, items, openComment);
	if (openComment && change?.created != null) openInspector(true);
	return change;
}

/** Marks the selected text with `kind` (one undo step) and clears the selection. */
export async function markSelection(tab: DocTab, kind: MarkupKind, openComment = markupOpensComment()) {
	const sel = tab.selection;
	if (!sel) return null;
	const [start, end] = ordered(sel.anchor, sel.focus);
	const pages = selectionRanges(tab.textMap(), start, end);
	tab.selection = null;
	return createMarkup(
		tab,
		kind,
		pages.map((p) => ({ page: p.page, body: { tool: 'textMarkup', kind, ranges: p.ranges, note: null } })),
		openComment
	);
}

/** Opens the inspector on the selected annotation; `focusNote` puts the cursor in its note. */
export function openInspector(focusNote: boolean) {
	app.annotationInspectorOpen = true;
	if (focusNote) app.focusNoteText = true;
}

export async function update(tab: DocTab, page: number, id: number, edit: AnnotationEditInput) {
	if (refuseIfLocked(tab)) return null;
	return app.apply(tab, { kind: 'updateAnnotation', page, id, edit });
}

/** Replies to annotation `parent` on `page` (ADR 0012); the author is the name from
 * Settings. One undo step, "Add reply". */
export async function addReply(tab: DocTab, page: number, parent: number, text: string) {
	if (refuseIfLocked(tab) || !text.trim()) return null;
	return app.apply(tab, { kind: 'addReply', page, parent, text });
}

/** Copies an annotation's comment (its note text) to the clipboard. */
export async function copyComment(text: string) {
	if (!text) return;
	try {
		await navigator.clipboard.writeText(text);
	} catch {
		app.notify({
			kind: 'error',
			message: 'The comment couldn’t be copied to the clipboard.',
			suggestion: 'Try again.'
		});
	}
}

export async function remove(tab: DocTab, page: number, id: number) {
	if (refuseIfLocked(tab)) return null;
	const a = tab.annotation(page, id);
	const replies = a ? tab.repliesTo(page, id).length : 0;
	if (replies > 0) {
		const choice = await app.ask({
			title: 'Delete annotation and replies?',
			message: replies === 1 ? 'This annotation has a reply, which is deleted with it.' : `This annotation has ${replies} replies, which are deleted with it.`,
			buttons: [
				{ id: 'delete', label: 'Delete', primary: true },
				{ id: 'cancel', label: 'Cancel' }
			],
			cancel: 'cancel'
		});
		if (choice !== 'delete') return null;
	}
	const change = await app.apply(tab, { kind: 'deleteAnnotation', page, id });
	if (change && tab.selectedAnnotation?.id === id) tab.selectedAnnotation = null;
	return change;
}

/**
 * Finishes the text box being typed: creates it, or changes the text of the one being
 * edited. An empty new box is dropped; emptying an existing one deletes it.
 */
export async function commitTextDraft(tab: DocTab) {
	const d = tab.draft;
	if (d?.kind !== 'text') return;
	tab.draft = null;
	const text = d.text.replace(/\s+$/, '');
	if (d.id === null) {
		if (!text.trim()) return;
		await create(tab, 'freeText', [{ page: d.page, body: { tool: 'freeText', rect: d.box, text, fontSize: d.fontSize } }]);
		return;
	}
	const a = tab.annotation(d.page, d.id);
	if (!a || a.contents === text) return;
	if (!text.trim()) await remove(tab, d.page, d.id);
	else await update(tab, d.page, d.id, { contents: text });
}

export function cancelTextDraft(tab: DocTab) {
	if (tab.draft?.kind === 'text') tab.draft = null;
}

/** Scans, shows what was found, and repairs on confirmation (section 5.3). */
export async function repair(tab: DocTab) {
	if (refuseIfLocked(tab)) return;
	let summary;
	try {
		summary = await scanAnnotationsForRepair(tab.id);
	} catch (e) {
		app.showError(toAppError(e));
		return;
	}
	if (summary.fixable === 0) {
		app.notify({
			kind: 'info',
			message:
				summary.unfixable > 0
					? `${plural(summary.unfixable, 'annotation has', 'annotations have')} problems Lectrix can’t fix, and nothing else needs repair.`
					: 'No annotations need repair.'
		});
		return;
	}
	const lines = summary.counts.map((c) => `${PROBLEM_SUMMARY[c.problem]}: ${c.count}`);
	const choice = await app.ask({
		title: 'Repair annotations',
		message: `${plural(summary.fixable, 'annotation', 'annotations')} can be repaired so that other apps show ${summary.fixable === 1 ? 'it' : 'them'} correctly. Their text, colour, author and position don’t change.`,
		detail:
			lines.join(' · ') +
			(summary.unfixable > 0 ? `. ${plural(summary.unfixable, 'annotation', 'annotations')} can’t be repaired.` : '.'),
		buttons: [
			{ id: 'repair', label: 'Repair', primary: true },
			{ id: 'cancel', label: 'Cancel' }
		],
		cancel: 'cancel'
	});
	if (choice !== 'repair') return;
	if (!(await app.allowEdit(tab))) return;
	try {
		tab.applyChange(await repairCommand(tab.id));
		app.notify({ kind: 'info', message: `Repaired ${plural(summary.fixable, 'annotation', 'annotations')}. Undo with Ctrl+Z.` });
	} catch (e) {
		app.showError(toAppError(e));
	}
}

function plural(n: number, one: string, many: string) {
	return `${n} ${n === 1 ? one : many}`;
}
