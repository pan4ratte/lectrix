import { describe, expect, it } from 'vitest';

import { platformOf } from '#lib/platform.ts';

import { macAlertLayout } from './alert.ts';

const ids = (layout: { buttons: { id: string }[] }) => layout.buttons.map((b) => b.id);

describe('macAlertLayout', () => {
	it('puts the main button at the right of two, Cancel at the left', () => {
		const layout = macAlertLayout([{ id: 'delete' }, { id: 'cancel' }], 'cancel');
		expect(ids(layout)).toEqual(['cancel', 'delete']);
		expect(layout.main?.id).toBe('delete');
		expect(layout.stacked).toBe(false);
	});

	it('takes the primary button as the main one wherever it is listed', () => {
		const layout = macAlertLayout([{ id: 'cancel' }, { id: 'save', primary: true }], 'cancel');
		expect(ids(layout)).toEqual(['cancel', 'save']);
		expect(layout.main?.id).toBe('save');
	});

	it('stacks three, the main one on top and Cancel at the bottom', () => {
		const layout = macAlertLayout([{ id: 'save', primary: true }, { id: 'discard' }, { id: 'cancel' }], 'cancel');
		expect(ids(layout)).toEqual(['save', 'discard', 'cancel']);
		expect(layout.stacked).toBe(true);
	});

	it('keeps a single button, and a question whose cancel is not a button', () => {
		expect(ids(macAlertLayout([{ id: 'ok' }], 'ok'))).toEqual(['ok']);
		expect(ids(macAlertLayout([{ id: 'restore', primary: true }, { id: 'later' }], 'escape'))).toEqual(['later', 'restore']);
	});
});

describe('platformOf', () => {
	it('names the system from the webview’s user agent', () => {
		expect(platformOf('Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36 Edg/141.0.0.0')).toBe('windows');
		expect(platformOf('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)')).toBe('macos');
		expect(platformOf('Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15')).toBe('linux');
	});
});
