// Guest power actions: confirmation (normal or destructive), the API call and the
// in-progress toast that turns into success or error when the Proxmox task ends.
import { api, errorMessage, unwrap, type Guest, type PowerAction } from '#lib/api/client.js';
import { duration, guestId } from '#lib/format.js';
import { live } from '#lib/live.svelte.js';
import { toasts } from '#lib/toasts.svelte.js';

export interface ConfirmRequest {
	guest: Guest;
	action: PowerAction;
	/** Names and health of linked services, for the dialog text. */
	linked: string[];
	resolve: (choice: PowerAction | null) => void;
}

const VERB: Record<PowerAction, [string, string, string]> = {
	// [button, in progress, done]
	start: ['Start', 'Starting', 'started'],
	shutdown: ['Shutdown', 'Shutting down', 'shut down'],
	reboot: ['Reboot', 'Rebooting', 'rebooted'],
	stop: ['Stop now', 'Stopping', 'stopped']
};

export function verb(action: PowerAction) {
	return VERB[action];
}

class Power {
	/** The open confirmation, rendered by `ConfirmDialog` in the app shell. */
	pending = $state<ConfirmRequest | null>(null);
	#toastFor = new Map<
		number,
		{ toast: number; started: number; name: string; action: PowerAction }
	>();

	constructor() {
		live.onAction(({ entry }) => {
			const t = this.#toastFor.get(entry.id);
			if (!t || entry.result === 'pending') return;
			this.#toastFor.delete(entry.id);
			const secs = (Date.now() - t.started) / 1000;
			if (entry.result === 'ok') {
				toasts.update(t.toast, {
					kind: 'success',
					title: `${t.name} ${VERB[t.action][2]} in ${duration(secs)}`
				});
			} else {
				toasts.update(t.toast, {
					kind: 'error',
					title: `${VERB[t.action][0].replace(' now', '')} failed: ${t.name}`,
					sub: entry.detail ?? undefined
				});
			}
		});
	}

	/** Ask for confirmation when needed. Resolves to the chosen action (possibly a safer one). */
	confirm(guest: Guest, action: PowerAction, linked: string[] = []): Promise<PowerAction | null> {
		if (action === 'start') return Promise.resolve('start');
		return new Promise((resolve) => {
			this.pending = { guest, action, linked, resolve };
		});
	}

	answer(choice: PowerAction | null) {
		const p = this.pending;
		this.pending = null;
		p?.resolve(choice);
	}

	/** Confirm (if needed) and run. Returns true if the action was started. */
	async run(guest: Guest, action: PowerAction, linked: string[] = []): Promise<boolean> {
		const chosen = await this.confirm(guest, action, linked);
		if (!chosen) return false;
		return this.send(guest, chosen);
	}

	/** Run without asking (the caller already confirmed, e.g. a bulk action). */
	async send(guest: Guest, action: PowerAction): Promise<boolean> {
		const toast = toasts.push({
			kind: 'progress',
			title: `${VERB[action][1]} ${guest.name}`,
			sub: `${guestId(guest.guest_type, guest.vmid)} · ${guest.node}`,
			mono: true
		});
		try {
			const res = unwrap(
				await api.POST('/proxmox/guests/{vmid}/power', {
					params: { path: { vmid: guest.vmid } },
					body: { action, confirm: action !== 'start' }
				})
			);
			toasts.update(toast, { sub: `${guestId(guest.guest_type, guest.vmid)} · ${res.upid}` });
			this.#toastFor.set(res.activity_id, {
				toast,
				started: Date.now(),
				name: guest.name,
				action
			});
			return true;
		} catch (e) {
			toasts.update(toast, {
				kind: 'error',
				title: `${VERB[action][0].replace(' now', '')} failed: ${guest.name}`,
				sub: errorMessage(e)
			});
			return false;
		}
	}
}

export const power = new Power();
