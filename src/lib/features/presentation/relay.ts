// Between a presentation and its audience screen (section 6.1, audience.rs): the window
// presenting sends what to show; the audience screen asks for it once it runs, and passes
// on the keys, clicks and swipes it gets, which the window presenting acts on.

/** What to show (a `StageState`), to the audience screen. */
export const STAGE_EVENT = 'presentation-stage';
/** What the audience screen gets (an `AudienceInput`), to the window presenting. */
export const INPUT_EVENT = 'presentation-input';

export type AudienceInput =
	/** It runs, and has nothing to show yet. */
	| { kind: 'ready' }
	| { kind: 'key'; key: string; code: string; shiftKey: boolean; ctrlKey: boolean; altKey: boolean; metaKey: boolean }
	| { kind: 'step'; direction: -1 | 1 }
	/** A link of the page shown, by its index among the page's links. */
	| { kind: 'link'; page: number; link: number };

const PREFIX = 'audience-';

/** The label of the audience screen of the window `owner` (`audience::label_for`). */
export function audienceLabel(owner: string): string {
	return `${PREFIX}${owner}`;
}

/** The window an audience screen `label` belongs to, or null for a window that is none. */
export function ownerOf(label: string): string | null {
	return label.startsWith(PREFIX) ? label.slice(PREFIX.length) : null;
}
