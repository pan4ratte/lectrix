// The buttons of a question (DialogHost) as a macOS alert places them (section 8): its main
// one (the primary, else the first that is not Cancel) is the one Return presses; Cancel is
// at the other end. Two buttons are side by side, the main one at the right; more are
// stacked, the main one on top and Cancel at the bottom.

export interface AlertButton {
	id: string;
	primary?: boolean;
}

export interface AlertLayout<T extends AlertButton> {
	/** In order: left to right, or top to bottom when stacked. */
	buttons: T[];
	/** The button Return presses, which has the keyboard focus as the alert opens. */
	main: T | null;
	stacked: boolean;
}

export function macAlertLayout<T extends AlertButton>(buttons: readonly T[], cancel: string): AlertLayout<T> {
	const main = buttons.find((b) => b.primary) ?? buttons.find((b) => b.id !== cancel) ?? buttons[0] ?? null;
	const others = buttons.filter((b) => b !== main && b.id !== cancel);
	const cancels = buttons.filter((b) => b !== main && b.id === cancel);
	const fromMain = [...(main ? [main] : []), ...others, ...cancels];
	const stacked = buttons.length > 2;
	return { buttons: stacked ? fromMain : fromMain.reverse(), main, stacked };
}
