// Back/forward history of jumps within a document (Alt+Left / Alt+Right, section 6.1).

export interface ViewPosition {
	page: number;
	/** How far down the page the top of the view is, 0 to 1. */
	offset: number;
}

const LIMIT = 100;

export class NavHistory {
	private back: ViewPosition[] = [];
	private forward: ViewPosition[] = [];

	get canGoBack() {
		return this.back.length > 0;
	}

	get canGoForward() {
		return this.forward.length > 0;
	}

	/** Records `from` before a jump. A new jump clears the forward list. */
	push(from: ViewPosition) {
		const last = this.back[this.back.length - 1];
		if (!last || last.page !== from.page || Math.abs(last.offset - from.offset) > 0.01) {
			this.back.push(from);
			if (this.back.length > LIMIT) this.back.shift();
		}
		this.forward = [];
	}

	/** The position to go back to, given where the view is now. */
	goBack(current: ViewPosition): ViewPosition | null {
		const target = this.back.pop();
		if (!target) return null;
		this.forward.push(current);
		return target;
	}

	goForward(current: ViewPosition): ViewPosition | null {
		const target = this.forward.pop();
		if (!target) return null;
		this.back.push(current);
		return target;
	}
}
