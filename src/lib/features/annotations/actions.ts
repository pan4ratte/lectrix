// Creating, changing, deleting and repairing annotations (sections 5.3 and 6.5). Every
// change is one operation in Rust, so it is one undo step.

import {
	repairAnnotations as repairCommand,
	scanAnnotationsForRepair,
	toAppError,
	type Annotation,
	type AnnotationBody,
	type AnnotationEditInput,
	type AnnotationUpdateInput,
	type NewAnnotationInput
} from '#lib/ipc/index.ts';
import { refuseRestricted } from '#lib/features/unprotect/actions.ts';
import { commandFor } from '#lib/shortcuts.ts';
import { app } from '#lib/stores/app.svelte.ts';
import type { AnnotationRef, DocTab } from '#lib/stores/doc.svelte.ts';

import { copySelection, copyText } from '#lib/features/viewer/actions.ts';
import { ordered } from '#lib/features/viewer/selection.ts';

import { commentKeeper } from './commentField.ts';
import { selectionRanges } from './geometry.ts';
import { markedText, marksText } from './listing.ts';
import { tools } from './state.svelte.ts';
import { capabilities, PROBLEM_SUMMARY, showsComment, type DrawTool, type MarkupKind } from './tools.ts';

/** Tells the user why nothing happens in a document that forbids annotating, or that is
 * open for reading only. */
export function refuseIfLocked(tab: DocTab): boolean {
	if (tab.flags.canAnnotate) return false;
	if (app.refuseReadOnly(tab)) return true;
	refuseRestricted(tab, 'This document’s security settings don’t allow adding or changing annotations.');
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
 * selected and the cursor goes in its comment: in the annotation list when that shows it,
 * otherwise in its comment panel.
 */
export async function createMarkup(
	tab: DocTab,
	kind: MarkupKind,
	items: { page: number; body: AnnotationBody }[],
	openComment = markupOpensComment()
) {
	const change = await create(tab, kind, items, openComment);
	if (openComment && change?.created != null) {
		openInspector(tab, true);
		madeForComment(tab, items[0]!.page, change.created, change.state.revision);
	}
	return change;
}

/** The annotation just made with the cursor put in its comment, while nothing else has
 * changed the document since (its revision then). */
let fresh: { tab: DocTab; page: number; id: number; revision: number } | null = null;

/** Annotation `id` on `page` was just made, at `revision`, and its comment opened for typing:
 * a new note, or new text markup that opens its comment (section 6.5). */
export function madeForComment(tab: DocTab, page: number, id: number, revision: number) {
	fresh = { tab, page, id, revision };
}

/**
 * Undo (Ctrl+Z) pressed in the comment of an annotation just made for it, while the comment
 * is still empty: the annotation is taken back, as Undo anywhere else would. Text typed
 * there is taken back by the field first, a step at a time, as in any field. Returns
 * whether it did that, so the field's key handler stops there.
 */
export function undoMadeForComment(tab: DocTab, page: number, id: number, event: KeyboardEvent): boolean {
	const field = event.currentTarget as HTMLTextAreaElement | null;
	if (field?.value !== '' || commandFor(event, false) !== 'undo') return false;
	const f = fresh;
	if (!f || f.tab !== tab || f.page !== page || f.id !== id || f.revision !== tab.state.revision) return false;
	event.preventDefault();
	event.stopPropagation();
	fresh = null;
	if (tab.selectedAnnotation?.page === page && tab.selectedAnnotation.id === id) tab.selectedAnnotation = null;
	tab.viewer?.focus();
	void app.undo(tab);
	return true;
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

/** The key `app.annotationPanel` knows the selected annotation by. */
export function selectedKey(tab: DocTab): string | null {
	const sel = tab.selectedAnnotation;
	return sel ? `${tab.key}:${sel.page}:${sel.id}` : null;
}

/** Whether the selected annotation shows its comment panel rather than its bar. */
export function panelOpen(tab: DocTab): boolean {
	const a = tab.selectedAnnotationInfo;
	const key = selectedKey(tab);
	if (!a || !key) return false;
	const panel = app.annotationPanel;
	return panel?.key === key ? panel.open : showsComment(a);
}

/** Shows (or, `open` false, closes) the selected annotation's comment panel. */
export function setPanel(tab: DocTab, open: boolean) {
	const key = selectedKey(tab);
	if (key) app.annotationPanel = { key, open };
}

/** The rows of the annotation list while it is on screen with `tab`'s annotations. */
function listRows(tab: DocTab) {
	const rows = app.annotationRows;
	return rows && rows.tab === tab && app.isShown('annotations') ? rows : null;
}

/**
 * Whether the annotation list is on screen with a row for the selected annotation. The row
 * then stands in for the comment panel and the bar, which the page doesn't show: it has
 * their controls and the comment (section 6.5). A reply shown in a thread has no row.
 */
export function listShowsSelected(tab: DocTab): boolean {
	const sel = tab.selectedAnnotation;
	return sel !== null && listRows(tab)?.has(sel.page, sel.id) === true;
}

/**
 * Whether the annotation list, while it is on screen, leaves `a` out: its search or its
 * filters don't match it. Such an annotation is not drawn on the page either, and the
 * pointer doesn't find it there (section 6.5). A reply goes with the annotation whose
 * thread it is in.
 */
export function hiddenByList(tab: DocTab, a: Annotation): boolean {
	const rows = listRows(tab);
	if (!rows?.filtering) return false;
	let row = a;
	// (A thread is a few replies deep at most; the limit is for a file whose replies loop.)
	for (let depth = 0; depth < 32 && row.replyTo !== null; depth++) {
		const parent = tab.annotation(row.page, row.replyTo);
		if (!parent) break;
		row = parent;
	}
	return !rows.has(row.page, row.id);
}

/** The annotations of `page` the list leaves out, as the page image is asked to leave
 * them out: their object numbers, in order (`pageUrl`). */
export function hiddenOnPage(tab: DocTab, page: number): number[] {
	if (!listRows(tab)?.filtering) return [];
	const ids = new Set<number>();
	for (const a of tab.annotationsOn(page)) if (hiddenByList(tab, a)) ids.add(a.id);
	return [...ids].sort((x, y) => x - y);
}

/**
 * Opens the comment of the selected annotation: its row in the annotation list when the
 * list shows it, otherwise its comment panel. `focusNote` puts the cursor in the comment.
 */
export function openInspector(tab: DocTab, focusNote: boolean) {
	setPanel(tab, true);
	if (!focusNote) return;
	if (listShowsSelected(tab)) app.focusListComment = true;
	else app.focusNoteText = true;
}

/**
 * Gives annotation `a` another colour or opacity (one undo step). With Settings' "Use the
 * last colour and opacity" on, new annotations get the colour, whatever their type, and new
 * ones of its type the opacity.
 */
export async function restyle(tab: DocTab, a: Annotation, patch: { color?: string; opacity?: number }) {
	const change = await update(tab, a.page, a.id, patch);
	if (change && tools.remember && a.kind !== null && a.replyTo === null) tools.setStyle(a.kind, patch);
	return change;
}

/**
 * What a double-click on the selected annotation opens: its comment in the annotation list
 * when the list shows it (its row, when the comment can't change), or else its comment panel
 * (`focusNote`: with the cursor in its note).
 */
export function openComment(tab: DocTab, focusNote: boolean) {
	openInspector(tab, focusNote);
	if (listShowsSelected(tab)) app.focusListComment = true;
}

/** Opens the Properties dialog of the selected annotation (its author and dates). */
export function openProperties() {
	app.annotationPropertiesOpen = true;
}

export async function update(tab: DocTab, page: number, id: number, edit: AnnotationEditInput) {
	if (refuseIfLocked(tab)) return null;
	return app.apply(tab, { kind: 'updateAnnotation', page, id, edit });
}

/** Keeps what is typed in the comment field of annotation `id` on `page` (`commentKeeper`):
 * one undo step when the field is left or goes with other text than the comment. */
export function keepsComment(tab: DocTab, page: number, id: number) {
	return commentKeeper(
		() => (app.tabs.includes(tab) ? (tab.annotation(page, id)?.contents ?? null) : null),
		(contents) => update(tab, page, id, { contents })
	);
}

/** Replies to annotation `parent` on `page` (ADR 0012); the author is the name from
 * Settings. One undo step, "Add reply". */
export async function addReply(tab: DocTab, page: number, parent: number, text: string) {
	if (refuseIfLocked(tab) || !text.trim()) return null;
	return app.apply(tab, { kind: 'addReply', page, parent, text });
}

/** Copies the text a text-markup annotation marks (the characters inside its quads, as the
 * annotation list's search reads them), honoring the document's copy permission. True if
 * something was copied. */
export async function copyMarkedText(tab: DocTab, a: Annotation): Promise<boolean> {
	if (!marksText(a)) return false;
	if (!tab.flags.canCopy) {
		refuseRestricted(tab, 'This document’s security settings don’t allow copying text.');
		return false;
	}
	const text = await tab.loadText(a.page);
	return copyText(text ? markedText(text.lines, a.quads) : '');
}

/** What Copy selection copies (the page's context menu, Ctrl+C): the selected text, or
 * else the text the selected annotation marks. */
export function copiesSelection(tab: DocTab): boolean {
	const a = tab.selectedAnnotationInfo;
	return !!tab.selection || (!!a && marksText(a));
}

/** Copy selection: see `copiesSelection`. */
export async function copyInContext(tab: DocTab): Promise<boolean> {
	if (tab.selection) return copySelection(tab);
	const a = tab.selectedAnnotationInfo;
	return a ? copyMarkedText(tab, a) : false;
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

// ----- annotations selected together (section 6.5) -----

/** Changes several annotations as one undo step. */
export async function updateMany(tab: DocTab, updates: AnnotationUpdateInput[]) {
	if (!updates.length || refuseIfLocked(tab)) return null;
	if (updates.length === 1) return update(tab, updates[0]!.page, updates[0]!.id, updates[0]!.edit);
	return app.apply(tab, { kind: 'updateAnnotations', updates });
}

/**
 * Gives the annotations selected together another colour or opacity (one undo step): those
 * whose colour can change. With Settings' "Use the last colour and opacity" on, new
 * annotations then get the colour, and new ones of their types the opacity.
 */
export async function restyleMany(tab: DocTab, group: readonly Annotation[], patch: { color?: string; opacity?: number }) {
	const styled = group.filter((a) => capabilities(a, tab.flags.canAnnotate).restyle);
	const change = await updateMany(
		tab,
		styled.map((a) => ({ page: a.page, id: a.id, edit: patch }))
	);
	if (change && tools.remember) {
		for (const kind of new Set(styled.flatMap((a) => (a.kind !== null && a.replyTo === null ? [a.kind] : [])))) {
			tools.setStyle(kind, patch);
		}
	}
	return change;
}

/**
 * Deletes the annotations selected together that can be deleted (one undo step), each with
 * its popup and replies; asks first when some of them have replies. Deselects them.
 */
export async function removeMany(tab: DocTab, refs: readonly AnnotationRef[]) {
	if (refuseIfLocked(tab)) return null;
	const doomed = refs.flatMap((r) => {
		const a = tab.annotation(r.page, r.id);
		return a && capabilities(a, tab.flags.canAnnotate).delete ? [a] : [];
	});
	if (!doomed.length) return null;
	if (doomed.length === 1) return remove(tab, doomed[0]!.page, doomed[0]!.id);
	// Replies selected with the annotation they answer go with it, so are not counted.
	const chosen = new Set(doomed.map((a) => `${a.page}:${a.id}`));
	const replies = doomed.reduce(
		(n, a) => n + tab.repliesTo(a.page, a.id).filter((r) => !chosen.has(`${r.page}:${r.id}`)).length,
		0
	);
	if (replies > 0) {
		const choice = await app.ask({
			title: `Delete ${doomed.length} annotations and their replies?`,
			message:
				replies === 1
					? 'One of these annotations has a reply, which is deleted with it.'
					: `These annotations have ${replies} replies, which are deleted with them.`,
			buttons: [
				{ id: 'delete', label: 'Delete', primary: true },
				{ id: 'cancel', label: 'Cancel' }
			],
			cancel: 'cancel'
		});
		if (choice !== 'delete') return null;
	}
	const change = await app.apply(tab, {
		kind: 'deleteAnnotations',
		annotations: doomed.map((a) => ({ page: a.page, id: a.id }))
	});
	if (change) tab.selectedAnnotation = null;
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
		await create(tab, 'freeText', [
			{ page: d.page, body: { tool: 'freeText', rect: d.box, text, fontSize: d.fontSize, turn: d.turn } }
		]);
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
		const undo = app.shortcut('undo');
		app.notify({
			kind: 'info',
			message: `Repaired ${plural(summary.fixable, 'annotation', 'annotations')}. Undo ${undo ? `with ${undo}` : 'from the Edit menu'}.`
		});
	} catch (e) {
		app.showError(toAppError(e));
	}
}

function plural(n: number, one: string, many: string) {
	return `${n} ${n === 1 ? one : many}`;
}
