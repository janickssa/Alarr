// Live updates over Server-Sent Events (`GET /api/v1/events`), merged into reactive state.
import type { ActivityEntry, Alert, Health, ServiceSnapshot } from '#lib/api/client.js';
import { connection } from '#lib/connection.svelte.js';

export interface HealthChange {
	service_id: string;
	previous: Health | null;
	health: Health;
	reason: string | null;
}

export interface ActionEvent {
	service_id: string | null;
	entry: ActivityEntry;
}

export interface AlertEvent {
	service_id: string | null;
	alert: Alert;
}

type ActionListener = (event: ActionEvent) => void;
type AlertListener = (event: AlertEvent) => void;

class Live {
	/** Latest snapshot per service id. */
	snapshots = $state<Record<string, ServiceSnapshot>>({});
	/** Bumped on every snapshot of a service; pages read it to refetch. */
	ticks = $state<Record<string, number>>({});
	/** Bumped on every health change (any service). */
	healthTick = $state(0);
	/** Bumped on every action event. */
	actionTick = $state(0);

	#source: EventSource | null = null;
	#listeners = new Set<ActionListener>();
	#alertListeners = new Set<AlertListener>();
	#retry: ReturnType<typeof setTimeout> | null = null;

	start() {
		if (this.#source || typeof EventSource === 'undefined') return;
		connection.live = 'connecting';
		const source = new EventSource('/api/v1/events');
		this.#source = source;
		source.onopen = () => {
			connection.live = 'open';
			connection.markReachable();
		};
		source.onerror = () => {
			connection.live = 'closed';
			// The browser retries by itself; also probe reachability so the banner can show.
			if (source.readyState === EventSource.CLOSED) {
				this.#source = null;
				this.#retry = setTimeout(() => this.start(), 5000);
			}
			fetch('/api/v1/info', { cache: 'no-store' })
				.then(() => connection.markReachable())
				.catch(() => connection.markUnreachable());
		};
		source.addEventListener('snapshot', (e) => {
			const snap = JSON.parse((e as MessageEvent).data) as ServiceSnapshot;
			this.snapshots[snap.service_id] = snap;
			this.ticks[snap.service_id] = (this.ticks[snap.service_id] ?? 0) + 1;
			connection.markReachable();
		});
		source.addEventListener('health', () => {
			this.healthTick++;
		});
		source.addEventListener('alert', (e) => {
			const event = JSON.parse((e as MessageEvent).data) as AlertEvent;
			for (const l of this.#alertListeners) l(event);
		});
		source.addEventListener('action', (e) => {
			const event = JSON.parse((e as MessageEvent).data) as ActionEvent;
			this.actionTick++;
			for (const l of this.#listeners) l(event);
		});
	}

	stop() {
		if (this.#retry) clearTimeout(this.#retry);
		this.#source?.close();
		this.#source = null;
		this.snapshots = {};
		connection.live = 'closed';
	}

	onAction(listener: ActionListener): () => void {
		this.#listeners.add(listener);
		return () => this.#listeners.delete(listener);
	}

	onAlert(listener: AlertListener): () => void {
		this.#alertListeners.add(listener);
		return () => this.#alertListeners.delete(listener);
	}
}

export const live = new Live();

/**
 * Re-run `load` whenever one of the services from `ids()` gets a new snapshot (and on
 * health/action events when `alsoOn` says so), at most every `minInterval` ms.
 * Call during component initialisation.
 */
export function refetchOnLive(
	ids: () => string[],
	load: () => Promise<unknown>,
	opts: { minInterval?: number; health?: boolean; actions?: boolean } = {}
) {
	const minInterval = opts.minInterval ?? 2000;
	let last = 0;
	let timer: ReturnType<typeof setTimeout> | null = null;
	let first = true;

	$effect(() => {
		for (const id of ids()) void live.ticks[id];
		if (opts.health) void live.healthTick;
		if (opts.actions) void live.actionTick;
		if (first) {
			first = false;
			return;
		}
		const wait = Math.max(0, last + minInterval - Date.now());
		if (timer) return;
		timer = setTimeout(() => {
			timer = null;
			last = Date.now();
			void load();
		}, wait);
	});

	$effect(() => () => {
		if (timer) clearTimeout(timer);
	});
}
