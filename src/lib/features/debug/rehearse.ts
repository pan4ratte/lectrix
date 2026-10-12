// The Debug menu's rehearsals (development builds, or LECTRIX_DEBUG=1): each shows a
// notification, notice or prompt as users see it, worded as the app words it, and changes
// nothing in Rust or on disk. An update rehearsal brings what CHANGELOG.md lists under
// Unreleased, so the next release's notes can be read as users will read them.
import { getVersion } from '@tauri-apps/api/app';

import { showUpdated } from '#lib/features/changelog/announce.ts';
import { changelogMarkdown, hasChanges, parseChangelog } from '#lib/features/changelog/changelog.ts';
import { hintFullScreen } from '#lib/features/fullscreen/actions.ts';
import { recoveryQuestion } from '#lib/features/recovery/recovery.ts';
import { notifyUpToDate, update } from '#lib/features/update/update.svelte.ts';
import type { PdfaLevel, RecoveredDocument, UpdateInfo } from '#lib/ipc/index.ts';
import { app } from '#lib/stores/app.svelte.ts';
import type { BannerKind } from '#lib/stores/doc.svelte.ts';

/** How long a rehearsed "Checking for updates…" lasts before the update shows. */
const CHECKING_MS = 1500;

function runningVersion(): Promise<string> {
	return getVersion().catch(() => '1.0.0');
}

/** The minor version after `version`: 1.0.0 -> 1.1.0. */
export function nextVersion(version: string): string {
	const [major = 0, minor = 0] = version.split(/[.-]/).map(Number);
	return `${major}.${minor + 1}.0`;
}

/** The update a rehearsal offers: the next version, bringing what `changelog` lists under
 * Unreleased; when that is empty, its newest version, as if the one before it ran. */
export function rehearsedUpdate(changelog: string, running: string): UpdateInfo {
	const releases = parseChangelog(changelog);
	const unreleased = releases.find((r) => r.version === null);
	if (unreleased && hasChanges(unreleased)) {
		const version = nextVersion(running);
		return { version, currentVersion: running, notes: changelog.replace(/^## Unreleased *$/m, `## ${version}`) };
	}
	const [newest, previous] = releases.filter((r) => r.version !== null);
	return {
		version: newest?.version ?? nextVersion(running),
		currentVersion: previous?.version ?? running,
		notes: changelog
	};
}

export type UpdateRehearsal = 'checking' | 'available' | 'downloading' | 'ready' | 'installed';

/** Shows the update notice at a stage; its buttons then carry on from there. */
export async function rehearseUpdate(stage: UpdateRehearsal) {
	const info = rehearsedUpdate(await changelogMarkdown(), await runningVersion());
	switch (stage) {
		case 'checking':
			update.simulate({ kind: 'checking' });
			setTimeout(() => {
				if (update.stage.kind === 'checking') update.simulate({ kind: 'available', info });
			}, CHECKING_MS);
			break;
		case 'available':
			update.simulate({ kind: 'available', info });
			break;
		case 'downloading':
			update.simulate({ kind: 'downloading', info, progress: null, stopping: false });
			break;
		case 'ready':
		case 'installed': {
			const installed = stage === 'installed';
			update.simulate({ kind: 'ready', info, installed, restarting: false }, installed);
			break;
		}
	}
}

export async function rehearseUpToDate() {
	notifyUpToDate(await runningVersion());
}

/** The errors Rust gives when GitHub can't be reached (update.rs). */
export function rehearseUpdateError(what: 'check' | 'download') {
	app.showError({
		message: what === 'check' ? 'Lectrix couldn’t check for updates.' : 'The update couldn’t be downloaded.',
		suggestion: 'Check your internet connection, then try again from Help > Check for updates.',
		code: 'general'
	});
}

export async function rehearseUpdated(where: 'start screen' | 'notification') {
	showUpdated(await runningVersion(), where);
}

export function rehearseInfo() {
	app.notify({ kind: 'info', message: 'Lectrix is combining files.', suggestion: 'Stop it first, or wait for it to finish.' });
}

/** What full screen says the first time it starts. */
export function rehearseFullScreen() {
	hintFullScreen();
}

/** A save refused because another program holds the file (ipc.rs, TargetLocked). */
export function rehearseError(withAction: boolean) {
	const name = app.active?.name ?? 'report.pdf';
	app.showError(
		{
			message: `${name} is open in another program, so it can't be replaced.`,
			suggestion: 'Close it in the other program (Acrobat locks files it has open), or use Save As.',
			code: 'fileLocked'
		},
		withAction ? { label: 'Save As…', run: () => rehearseInfoAbout('Save As would open now.') } : undefined
	);
}

function rehearseInfoAbout(what: string) {
	app.notify({ kind: 'info', message: `Rehearsal: ${what}`, suggestion: 'Nothing was changed.' });
}

/** The banner of a document another program changed or removed (needs one open). */
export function rehearseBanner(kind: BannerKind) {
	if (app.active) app.active.banner = kind;
}

/** The notification after a save that took the PDF/A claim out of a file. */
export function rehearsePdfaRemoved() {
	app.notifyPdfaRemoved(app.active?.name ?? 'report.pdf', { part: 1, conformance: 'B' });
}

/** The documents shown as PDF/A files by a rehearsal, with what they really claim. */
const rehearsedPdfa = new Map<number, PdfaLevel | null>();

/**
 * The banner of a file that claims PDF/A, opened for reading only (needs a document open):
 * the document can't be changed until Enable editing, as with a real one.
 */
export function rehearsePdfa() {
	const tab = app.active;
	if (!tab || tab.readOnly) return;
	if (!rehearsedPdfa.has(tab.id)) rehearsedPdfa.set(tab.id, tab.fileFlags.pdfa);
	tab.fileFlags = { ...tab.fileFlags, pdfa: tab.fileFlags.pdfa ?? { part: 1, conformance: 'B' } };
	tab.readOnly = true;
}

/** The offer to restore unsaved changes a crash left behind, for one or several documents. */
export async function rehearseRecovery(count: 1 | 3) {
	const now = Date.now();
	const found: RecoveredDocument[] = [
		{ slot: 'a', name: 'Annual report.pdf', folder: 'C:\\Users\\you\\Documents', savedAt: now - 5 * 60_000, exists: true },
		{ slot: 'b', name: 'Thesis draft.pdf', folder: 'D:\\Studies', savedAt: now - 3 * 3_600_000, exists: true },
		{ slot: 'c', name: 'Scan 0042.pdf', folder: 'C:\\Users\\you\\Downloads', savedAt: now - 26 * 3_600_000, exists: false }
	].slice(0, count);
	const question = recoveryQuestion(found, now);
	const choice = await app.ask(question);
	const label = question.buttons.find((b) => b.id === choice)?.label.replace(/…$/, '') ?? choice;
	rehearseInfoAbout(`you chose “${label}”.`);
}

/** Takes every rehearsal (and any real notification) off the screen. */
export function clearRehearsals() {
	app.toasts = [];
	app.updatedTo = null;
	update.endRehearsal();
	for (const tab of app.tabs) {
		tab.banner = null;
		if (!rehearsedPdfa.has(tab.id)) continue;
		tab.readOnly = false;
		tab.fileFlags = { ...tab.fileFlags, pdfa: rehearsedPdfa.get(tab.id) ?? null };
	}
	rehearsedPdfa.clear();
}
