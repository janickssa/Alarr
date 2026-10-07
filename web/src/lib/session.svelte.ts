// Signed-in user and the list of services (drives the sidebar and categories).
import { alerts } from '#lib/alerts.svelte.js';
import { api, unwrap, type Me, type ServiceView } from '#lib/api/client.js';
import { live } from '#lib/live.svelte.js';

class Session {
	me = $state<Me | null>(null);
	services = $state<ServiceView[]>([]);
	servicesLoaded = $state(false);

	get isOwner() {
		return this.me?.user.role === 'owner';
	}

	/** Whether the user may run power and service actions on a service. */
	canAct(serviceId: string): boolean {
		const u = this.me?.user;
		if (!u) return false;
		if (u.role !== 'guest') return true;
		return u.can_act && u.allowed_services.includes(serviceId);
	}

	get proxmox(): ServiceView | undefined {
		return this.services.find((s) => s.kind === 'proxmox' && s.enabled);
	}

	async loadMe(): Promise<Me | null> {
		const res = await api.GET('/auth/me');
		this.me = res.response.ok ? (res.data ?? null) : null;
		return this.me;
	}

	/** After sign-in: remember the user, open the live stream and load services. */
	async start(me: Me) {
		this.me = me;
		live.start();
		alerts.start();
		await this.loadServices().catch(() => {});
	}

	async loadServices() {
		this.services = unwrap(await api.GET('/services'));
		this.servicesLoaded = true;
	}

	clear() {
		alerts.stop();
		this.me = null;
		this.services = [];
		this.servicesLoaded = false;
	}
}

export const session = new Session();
