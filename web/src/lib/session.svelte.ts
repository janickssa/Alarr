// Signed-in user and the list of services (drives the sidebar and categories).
import { alerts } from '#lib/alerts.svelte.js';
import { api, unwrap, type Me, type ServiceView } from '#lib/api/client.js';
import { live } from '#lib/live.svelte.js';

/** A failed `/auth/me` that isn't "signed out": no network, or an HTTP error. */
export class ConnectError extends Error {
	constructor(public status?: number) {
		super(status ? `HTTP ${status}` : 'no answer');
	}
}

const REMEMBER_KEY = 'quorum.signedIn';

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

	/**
	 * The signed-in user, or `null` when the server says we're not signed in (401). Any
	 * other failure (no network yet, proxy error, server restarting) throws, so a slow
	 * VPN at app start doesn't look like being signed out.
	 */
	async loadMe(): Promise<Me | null> {
		let res;
		try {
			res = await api.GET('/auth/me');
		} catch {
			throw new ConnectError();
		}
		if (res.response.status === 401) {
			this.me = null;
			return null;
		}
		if (!res.response.ok || !res.data) throw new ConnectError(res.response.status);
		this.me = res.data;
		return this.me;
	}

	/**
	 * Whether this browser has been signed in and not signed out on purpose. While true the
	 * app never sends you to the sign-in page by itself; it explains what's wrong instead.
	 */
	get remembered(): boolean {
		try {
			return localStorage.getItem(REMEMBER_KEY) === '1';
		} catch {
			return false;
		}
	}

	set remembered(on: boolean) {
		try {
			if (on) localStorage.setItem(REMEMBER_KEY, '1');
			else localStorage.removeItem(REMEMBER_KEY);
		} catch {
			// Private mode or storage blocked: fall back to the old behaviour.
		}
	}

	/** After sign-in: remember the user, open the live stream and load services. */
	async start(me: Me) {
		this.me = me;
		this.remembered = true;
		live.start();
		alerts.start();
		await this.loadServices().catch(() => {});
	}

	async loadServices() {
		this.services = unwrap(await api.GET('/services'));
		this.servicesLoaded = true;
	}

	/** Sign out (on a paired PWA this also unpairs the device) and forget local state. */
	async signOut() {
		this.remembered = false;
		await api.POST('/auth/logout').catch(() => {});
		live.stop();
		this.clear();
	}

	clear() {
		alerts.stop();
		this.me = null;
		this.services = [];
		this.servicesLoaded = false;
	}
}

export const session = new Session();
