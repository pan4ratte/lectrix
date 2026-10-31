import { describe, expect, it } from 'vitest';

import { followedHit } from './searchfollow.ts';

// Pages 1000 px tall, end to end; each match at its page's top plus its offset.
const hits = [
	{ page: 0, at: 100 },
	{ page: 2, at: 200 },
	{ page: 2, at: 800 },
	{ page: 3, at: 500 },
	{ page: 6, at: 50 }
];
const y = (i: number) => hits[i]!.page * 1000 + hits[i]!.at;
const none = { index: -1, between: false };

/** The view from `top`, 1000 px tall. */
const view = (top: number, current = none) =>
	followedHit(hits, [Math.floor(top / 1000), Math.floor((top + 1000) / 1000)], y, top, top + 1000, current);

describe('followedHit', () => {
	it('counts the last match on screen', () => {
		expect(view(2000)).toEqual({ index: 2, between: false });
		expect(view(2700)).toEqual({ index: 3, between: false });
	});

	it('keeps the match shown while it is on screen', () => {
		expect(view(2000, { index: 1, between: false })).toEqual({ index: 1, between: false });
		// Scrolled off: the last on screen.
		expect(view(2300, { index: 1, between: false })).toEqual({ index: 2, between: false });
	});

	it('with none on screen, counts the last one above as passed', () => {
		expect(view(4000)).toEqual({ index: 3, between: true });
		expect(view(4000, { index: 3, between: true })).toEqual({ index: 3, between: true });
	});

	it('counts none above every match', () => {
		expect(view(-1000)).toEqual(none);
		expect(view(-1000, { index: 0, between: false })).toEqual(none);
	});

	it('skips matches without a place', () => {
		const out = followedHit(hits, [2, 3], (i) => (i === 3 ? null : y(i)), 2000, 3600, none);
		expect(out).toEqual({ index: 2, between: false });
	});
});
