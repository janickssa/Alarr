// Toasts: bottom-right on desktop, 5 s timeout. In-progress toasts stay until they
// resolve, then change to success or error in place.

export type ToastKind = 'progress' | 'success' | 'error' | 'info';

export interface Toast {
	id: number;
	kind: ToastKind;
	title: string;
	sub?: string;
	mono?: boolean;
	action?: { label: string; run: () => void };
}

const TIMEOUT = 5000;

class Toasts {
	items = $state<Toast[]>([]);
	#next = 1;
	#timers = new Map<number, ReturnType<typeof setTimeout>>();

	push(toast: Omit<Toast, 'id'>): number {
		const id = this.#next++;
		this.items.push({ ...toast, id });
		this.#arm(id, toast.kind);
		return id;
	}

	update(id: number, patch: Partial<Omit<Toast, 'id'>>) {
		const t = this.items.find((x) => x.id === id);
		if (!t) {
			if (patch.kind && patch.title) this.push(patch as Omit<Toast, 'id'>);
			return;
		}
		Object.assign(t, patch);
		if (patch.kind) this.#arm(id, patch.kind);
	}

	dismiss(id: number) {
		this.items = this.items.filter((t) => t.id !== id);
		clearTimeout(this.#timers.get(id));
		this.#timers.delete(id);
	}

	#arm(id: number, kind: ToastKind) {
		clearTimeout(this.#timers.get(id));
		if (kind === 'progress') return;
		this.#timers.set(
			id,
			setTimeout(() => this.dismiss(id), TIMEOUT)
		);
	}
}

export const toasts = new Toasts();
