<script lang="ts">
	// Global search over services, Proxmox guests and media (movies, series, torrents). "/" focuses it from anywhere.
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { api, type Guest, type Schemas } from '#lib/api/client.js';
	import { guestId } from '#lib/format.js';
	import { session } from '#lib/session.svelte.js';
	import HealthDot from '../HealthDot.svelte';
	import Icon from '../Icon.svelte';
	import SearchField from '../SearchField.svelte';
	import ServiceIcon from '../ServiceIcon.svelte';

	let { input = $bindable<HTMLInputElement | undefined>() }: { input?: HTMLInputElement } =
		$props();

	let query = $state('');
	let open = $state(false);
	let active = $state(0);
	let guests = $state<Guest[]>([]);
	let guestsAt = 0;

	async function loadGuests() {
		if (!session.proxmox || Date.now() - guestsAt < 30_000) return;
		guestsAt = Date.now();
		const res = await api.GET('/proxmox/guests');
		if (res.data) guests = res.data;
	}

	type MediaHit = Schemas['MediaSearchResult'];
	let media = $state<MediaHit[]>([]);
	let mediaSeq = 0;

	// Media search hits the backend, so debounce it and drop stale responses.
	$effect(() => {
		const q = query.trim();
		const seq = ++mediaSeq;
		if (q.length < 2) {
			media = [];
			return;
		}
		const t = setTimeout(async () => {
			const res = await api.GET('/media/search', { params: { query: { q } } });
			if (seq === mediaSeq) media = res.data ?? [];
		}, 250);
		return () => clearTimeout(t);
	});

	const MEDIA_ICON: Record<string, string> = {
		movie: 'ph-film-strip',
		series: 'ph-television-simple',
		torrent: 'ph-download-simple'
	};

	type Result = {
		key: string;
		label: string;
		sub: string;
		href: string;
		kind: 'service' | 'guest' | 'media';
		glyph?: string;
		icon?: { icon: string; hue: number; chroma: number };
		health?: 'healthy' | 'down' | 'warning' | 'unknown' | 'disabled' | 'in_progress';
	};

	const results = $derived.by((): Result[] => {
		const q = query.trim().toLowerCase();
		if (!q) return [];
		const svc: Result[] = session.services
			.filter((s) => `${s.name} ${s.id} ${s.preset ?? ''}`.toLowerCase().includes(q))
			.slice(0, 6)
			.map((s) => ({
				key: `s:${s.id}`,
				label: s.name,
				sub: s.linked_guest ? guestId(s.linked_guest.guest_type, s.linked_guest.vmid) : s.id,
				href: resolve('/services/[id]', { id: s.id }),
				kind: 'service',
				icon: { icon: s.icon, hue: s.accent_hue, chroma: s.accent_chroma },
				health: s.health
			}));
		const gst: Result[] = guests
			.filter((g) => `${g.name} ${g.vmid} ${g.tags.join(' ')}`.toLowerCase().includes(q))
			.slice(0, 6)
			.map((g) => ({
				key: `g:${g.vmid}`,
				label: g.name,
				sub: `${guestId(g.guest_type, g.vmid)} · ${g.node}`,
				href: resolve('/proxmox') + `?guest=${g.vmid}`,
				kind: 'guest',
				health: g.status === 'running' ? 'healthy' : g.status === 'stopped' ? 'down' : 'unknown'
			}));
		const names = Object.fromEntries(session.services.map((s) => [s.id, s.name]));
		const med: Result[] = media.slice(0, 8).map((m) => ({
			key: `m:${m.service_id}:${m.id}`,
			label: m.title,
			sub: [m.subtitle, names[m.service_id] ?? m.service_id].filter(Boolean).join(' · '),
			href: resolve('/services/[id]', { id: m.service_id }),
			kind: 'media',
			glyph: MEDIA_ICON[m.kind] ?? 'ph-film-strip'
		}));
		return [...svc, ...gst, ...med];
	});

	$effect(() => {
		void query;
		active = 0;
	});

	function go(r: Result) {
		open = false;
		query = '';
		input?.blur();
		// Hrefs come from resolve() above.
		void goto(r.href);
	}

	function onkeydown(e: KeyboardEvent) {
		if (e.key === 'ArrowDown') {
			e.preventDefault();
			active = Math.min(active + 1, results.length - 1);
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			active = Math.max(active - 1, 0);
		} else if (e.key === 'Enter' && results[active]) {
			e.preventDefault();
			go(results[active]);
		} else if (e.key === 'Escape') {
			open = false;
			query = '';
			input?.blur();
		}
	}
</script>

<div
	class="gs"
	onfocusin={() => {
		open = true;
		void loadGuests();
	}}
	onfocusout={(e) => {
		if (!(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node)) open = false;
	}}
>
	<SearchField
		bind:value={query}
		bind:input
		shortcut
		width="min(360px, 100%)"
		label="Search services, containers and media"
		placeholder="Search services, containers, media…"
		{onkeydown}
	/>
	{#if open && query.trim()}
		<ul class="results" role="listbox" aria-label="Search results">
			{#each results as r, i (r.key)}
				<li role="option" aria-selected={i === active}>
					<button type="button" onmousedown={(e) => e.preventDefault()} onclick={() => go(r)}>
						{#if r.icon}
							<ServiceIcon icon={r.icon.icon} hue={r.icon.hue} chroma={r.icon.chroma} size={24} />
						{:else}
							<span class="cube"><Icon name={r.glyph ?? 'ph-cube'} /></span>
						{/if}
						<span class="lbl">{r.label}<span class="sub mono">{r.sub}</span></span>
						{#if r.health}<HealthDot
								health={r.health}
								label={r.kind === 'guest'
									? r.health === 'healthy'
										? 'Running'
										: 'Stopped'
									: undefined}
							/>{/if}
					</button>
				</li>
			{:else}
				<li class="none">No matches</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.gs {
		position: relative;
	}
	.results {
		position: absolute;
		top: calc(100% + 4px);
		left: 0;
		width: 420px;
		max-width: calc(100vw - 2 * var(--gutter));
		z-index: 40;
		list-style: none;
		margin: 0;
		padding: 4px;
		background: var(--color-surface);
		border-radius: var(--radius-md);
		box-shadow: var(--shadow-md);
	}
	button {
		width: 100%;
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 6px 8px;
		border: 0;
		background: none;
		border-radius: 6px;
		cursor: pointer;
		text-align: left;
		font-size: 13px;
	}
	li[aria-selected='true'] button,
	button:hover {
		background: color-mix(in srgb, var(--color-text) 7%, transparent);
	}
	.lbl {
		flex: 1;
		display: flex;
		flex-direction: column;
		min-width: 0;
	}
	.sub {
		font-size: 11px;
		color: var(--color-neutral-500);
	}
	.cube {
		width: 24px;
		height: 24px;
		display: grid;
		place-items: center;
		color: var(--color-neutral-400);
	}
	.none {
		padding: 8px;
		font-size: 13px;
		color: var(--color-neutral-400);
	}
</style>
