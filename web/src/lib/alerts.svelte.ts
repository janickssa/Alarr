// Alerts in the web app: active count for the sidebar, this user's mutes, and a toast for
// each new alert that isn't muted.
import { goto } from '$app/navigation';
import { resolve } from '$app/paths';
import { api, type AlertType } from '#lib/api/client.js';
import { live } from '#lib/live.svelte.js';
import { toasts } from '#lib/toasts.svelte.js';

class Alerts {
	active = $state(0);
	mutes = $state<AlertType[]>([]);
	#stop: (() => void) | null = null;

	async refresh() {
		const [active, mutes] = await Promise.all([
			api.GET('/alerts', { params: { query: { active: true, limit: 200 } } }),
			api.GET('/alerts/mutes')
		]);
		if (active.data) this.active = active.data.length;
		if (mutes.data) this.mutes = mutes.data.types;
	}

	start() {
		void this.refresh();
		this.#stop?.();
		this.#stop = live.onAlert(({ alert }) => {
			void this.refresh();
			if (alert.resolved_at) return;
			if (this.mutes.includes(alert.alert_type as AlertType)) return;
			toasts.push({
				kind: alert.severity === 'critical' ? 'error' : 'info',
				title: alert.title,
				sub: alert.message,
				action: { label: 'View', run: () => void goto(resolve('/alerts')) }
			});
		});
	}

	stop() {
		this.#stop?.();
		this.#stop = null;
		this.active = 0;
		this.mutes = [];
	}
}

export const alerts = new Alerts();
