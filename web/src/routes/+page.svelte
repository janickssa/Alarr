<script lang="ts">
	// Home: status banner, problems with quick actions, Proxmox node strip and one
	// highlight card per enabled service (reorderable and hideable in edit mode).
	import { resolve } from '$app/paths';
	import { onMount } from 'svelte';
	import { api, errorMessage, unwrap, type Overview } from '#lib/api/client.js';
	import EmptyState from '#lib/components/EmptyState.svelte';
	import ErrorState from '#lib/components/ErrorState.svelte';
	import HealthBadge from '#lib/components/HealthBadge.svelte';
	import HealthCheckCard from '#lib/components/HealthCheckCard.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import LargeCard from '#lib/components/LargeCard.svelte';
	import ResourceBar from '#lib/components/ResourceBar.svelte';
	import ServiceCard from '#lib/components/ServiceCard.svelte';
	import ServiceIcon from '#lib/components/ServiceIcon.svelte';
	import SkeletonCard from '#lib/components/SkeletonCard.svelte';
	import Switch from '#lib/components/Switch.svelte';
	import { bytes, count, pct, rate, usedOf } from '#lib/format.js';
	import { HEALTH } from '#lib/health.js';
	import { live, refetchOnLive } from '#lib/live.svelte.js';
	import type { QbittorrentData, TechnitiumData } from '#lib/api/client.js';
	import { power } from '#lib/power.svelte.js';
	import { session } from '#lib/session.svelte.js';
	import { toasts } from '#lib/toasts.svelte.js';

	let data = $state<Overview | null>(null);
	let error = $state('');
	let editing = $state(false);
	let order = $state<string[]>([]);
	let hidden = $state<string[]>([]);

	async function load() {
		try {
			data = unwrap(await api.GET('/overview'));
			error = '';
		} catch (e) {
			error = errorMessage(e);
		}
	}

	onMount(load);
	refetchOnLive(() => session.services.map((s) => s.id), load, { health: true, actions: true });

	const headline = $derived.by(() => {
		if (!data) return null;
		const s = data.status;
		if (s.total === 0) return { health: 'unknown' as const, text: 'No services yet' };
		if (s.down > 0) return { health: 'down' as const, text: s.message };
		if (s.warning > 0) return { health: 'warning' as const, text: s.message };
		return { health: 'healthy' as const, text: s.message };
	});

	async function quick(p: Overview['problems'][number]) {
		if (p.quick_action?.kind === 'start_guest' && p.vmid != null) {
			try {
				const detail = unwrap(
					await api.GET('/proxmox/guests/{vmid}', { params: { path: { vmid: p.vmid } } })
				);
				await power.run(detail.guest, 'start');
			} catch (e) {
				toasts.push({ kind: 'error', title: 'Start failed', sub: errorMessage(e) });
			}
		} else if (p.service_id) {
			await api.POST('/services/{id}/actions/{action}', {
				params: { path: { id: p.service_id, action: 'refresh' } },
				body: { confirm: false }
			});
			toasts.push({ kind: 'info', title: `Checking ${p.title} again` });
		}
	}

	async function startEdit() {
		const s = unwrap(await api.GET('/settings'));
		const ids = session.services.filter((x) => x.enabled).map((x) => x.id);
		order = [
			...s.settings.home_order.filter((id) => ids.includes(id)),
			...ids.filter((id) => !s.settings.home_order.includes(id))
		];
		hidden = s.settings.home_hidden;
		editing = true;
	}

	function move(i: number, d: number) {
		const j = i + d;
		if (j < 0 || j >= order.length) return;
		[order[i], order[j]] = [order[j], order[i]];
	}

	async function saveEdit() {
		try {
			unwrap(await api.PUT('/settings', { body: { home_order: order, home_hidden: hidden } }));
			editing = false;
			await load();
		} catch (e) {
			toasts.push({ kind: 'error', title: "Couldn't save the layout", sub: errorMessage(e) });
		}
	}

	const onlineNodes = $derived(data?.nodes.filter((n) => n.online) ?? []);
	const avgCpu = $derived(
		onlineNodes.length
			? onlineNodes.reduce((a, n) => a + (n.cpu_usage ?? 0), 0) / onlineNodes.length
			: null
	);
	const ramFrac = $derived.by(() => {
		const total = onlineNodes.reduce((a, n) => a + (n.mem_total ?? 0), 0);
		return total ? onlineNodes.reduce((a, n) => a + (n.mem_used ?? 0), 0) / total : null;
	});
	function qbt(id: string): QbittorrentData | null {
		const d = live.snapshots[id]?.data;
		return d?.type === 'qbittorrent' ? d.value : null;
	}

	function dns(id: string): TechnitiumData | null {
		const d = live.snapshots[id]?.data;
		return d?.type === 'technitium' ? d.value : null;
	}

	const byId = $derived(Object.fromEntries(session.services.map((s) => [s.id, s])));
</script>

<svelte:head><title>Home</title></svelte:head>

<div class="page">
	{#if error && !data}
		<ErrorState title="Can't load the overview" detail={error} onretry={load} />
	{:else if !data || !headline}
		<div class="skeleton" style="height:40px;width:320px"></div>
		<div class="grid">
			{#each Array(6), i (i)}<SkeletonCard />{/each}
		</div>
	{:else}
		<div class="page-header">
			<h1 class="page-title status" style:--fg={HEALTH[headline.health].fg}>
				<Icon
					name={HEALTH[headline.health].icon}
					fill
					size={30}
					color={HEALTH[headline.health].fg}
				/>
				{headline.text}
			</h1>
			<div class="spacer"></div>
			{#if session.isOwner && data.highlights.length + (data.status.total - data.highlights.length) > 0}
				{#if editing}
					<button class="btn btn-secondary" onclick={() => (editing = false)}>Cancel</button>
					<button class="btn btn-primary" onclick={saveEdit}><Icon name="ph-check" />Done</button>
				{:else}
					<button class="btn btn-secondary" onclick={startEdit}
						><Icon name="ph-pencil-simple" />Edit</button
					>
				{/if}
			{/if}
		</div>

		{#if data.status.total === 0}
			<EmptyState
				icon="ph-plus-circle"
				title="No services yet"
				detail="Add Proxmox and health checks to see them here."
			>
				{#if session.isOwner}
					<a class="btn btn-primary" href={resolve('/settings/services/new')}
						><Icon name="ph-plus" />Add a service</a
					>
				{/if}
			</EmptyState>
		{/if}

		{#if editing}
			<section class="card edit" aria-label="Home layout">
				<span class="muted"
					>Reorder or hide cards. Hidden services still show in the sidebar and Health.</span
				>
				<ul>
					{#each order as id, i (id)}
						{@const s = byId[id]}
						{#if s}
							<li>
								<ServiceIcon icon={s.icon} hue={s.accent_hue} chroma={s.accent_chroma} size={24} />
								<span class="spacer">{s.name}</span>
								<button
									class="btn btn-ghost btn-icon"
									aria-label="Move {s.name} up"
									disabled={i === 0}
									onclick={() => move(i, -1)}><Icon name="ph-arrow-up" /></button
								>
								<button
									class="btn btn-ghost btn-icon"
									aria-label="Move {s.name} down"
									disabled={i === order.length - 1}
									onclick={() => move(i, 1)}><Icon name="ph-arrow-down" /></button
								>
								<Switch
									label="Show {s.name} on Home"
									checked={!hidden.includes(id)}
									onchange={(v) => (hidden = v ? hidden.filter((h) => h !== id) : [...hidden, id])}
								/>
							</li>
						{/if}
					{/each}
				</ul>
			</section>
		{/if}

		{#if data.problems.length > 0}
			<section class="problems" aria-labelledby="problems-h">
				<h2 id="problems-h" class="section-title">Problems</h2>
				<div class="card list">
					{#each data.problems as p (p.service_id ?? p.title)}
						{@const s = p.service_id ? byId[p.service_id] : undefined}
						<div class="problem">
							{#if s}<ServiceIcon icon={s.icon} hue={s.accent_hue} chroma={s.accent_chroma} />{/if}
							<div class="ptxt">
								{#if p.service_id}
									<a href={resolve('/services/[id]', { id: p.service_id })}>{p.title}</a>
								{:else}<span>{p.title}</span>{/if}
								<span class="muted">{p.reason ?? ''}</span>
							</div>
							<HealthBadge health={p.health} small />
							{#if p.quick_action && (!p.service_id || session.canAct(p.service_id) || p.quick_action.kind === 'retry')}
								<button class="btn btn-secondary btn-sm" onclick={() => quick(p)}>
									<Icon
										name={p.quick_action.kind === 'start_guest' ? 'ph-play' : 'ph-arrow-clockwise'}
									/>
									{p.quick_action.label}
								</button>
							{/if}
						</div>
					{/each}
				</div>
			</section>
		{/if}

		{#if data.nodes.length > 0}
			<section aria-labelledby="nodes-h">
				<div class="row sec-head">
					<h2 id="nodes-h" class="section-title">Nodes</h2>
					<a class="btn btn-ghost btn-sm" href={resolve('/proxmox')}
						>Proxmox<Icon name="ph-caret-right" /></a
					>
				</div>
				<div class="nodes">
					{#each data.nodes as n (n.name)}
						<a class="card node" href={resolve('/proxmox')}>
							<div class="row">
								<span class="mono">{n.name}</span>
								<span class="spacer"></span>
								<HealthBadge
									health={n.online ? 'healthy' : 'down'}
									label={n.online ? 'Online' : 'Offline'}
									small
								/>
							</div>
							<ResourceBar label="CPU" fraction={n.cpu_usage} value={pct(n.cpu_usage)} compact />
							<ResourceBar
								label="RAM"
								fraction={n.mem_total ? (n.mem_used ?? 0) / n.mem_total : null}
								value={usedOf(n.mem_used, n.mem_total)}
								compact
							/>
							<span class="faint caption">{n.guests_running} guests running</span>
						</a>
					{/each}
				</div>
			</section>
		{/if}

		{#if data.highlights.length > 0}
			<section aria-labelledby="svc-h">
				<h2 id="svc-h" class="visually-hidden">Services</h2>
				<div class="grid cards">
					{#each data.highlights as h (h.service.id)}
						{@const href =
							h.service.kind === 'proxmox'
								? resolve('/proxmox')
								: resolve('/services/[id]', { id: h.service.id })}
						{#if h.service.kind === 'proxmox'}
							<LargeCard
								service={h.service}
								{href}
								stats={[
									{
										label: 'Nodes',
										value: `${data.nodes.filter((n) => n.online).length} / ${data.nodes.length}`
									},
									{
										label: 'Guests running',
										value: `${data.nodes.reduce((a, n) => a + n.guests_running, 0)}`
									},
									{ label: 'CPU', value: pct(avgCpu) },
									{ label: 'RAM', value: pct(ramFrac) }
								]}
							/>
						{:else if h.service.kind === 'qbittorrent' && qbt(h.service.id)}
							{@const q = qbt(h.service.id)!}
							<LargeCard
								service={h.service}
								{href}
								stats={[
									{ label: 'Down', value: rate(q.dl_speed) },
									{ label: 'Up', value: rate(q.up_speed) },
									{ label: 'Active', value: `${q.counts.downloading}` },
									{ label: 'Free', value: bytes(q.free_space) }
								]}
								chart={q.history.map((p) => p.dl)}
								chartLabel="Download · 10 min"
							/>
						{:else if h.service.kind === 'technitium' && dns(h.service.id)}
							{@const d = dns(h.service.id)!}
							<LargeCard
								service={h.service}
								{href}
								stats={[
									{ label: 'Queries · 24 h', value: count(d.total_queries) },
									{
										label: 'Blocked',
										value: d.total_queries ? pct(d.blocked / d.total_queries) : '–'
									},
									{ label: 'Clients', value: `${d.clients}` }
								]}
								chart={d.chart.map((p) => p.blocked)}
								chartLabel="Blocked · 24 h"
							/>
						{:else if h.service.level === 'health_check'}
							<HealthCheckCard service={h.service} uptime7d={h.detail} {href} />
						{:else}
							<ServiceCard service={h.service} headline={h.headline} detail={h.detail} {href} />
						{/if}
					{/each}
				</div>
			</section>
		{/if}
	{/if}
</div>

<style>
	.status {
		display: flex;
		align-items: center;
		gap: 12px;
	}
	.sec-head {
		margin-bottom: 8px;
	}
	.problems .section-title,
	section > .section-title {
		margin-bottom: 8px;
	}
	.list {
		padding: 4px 0;
		gap: 0;
	}
	.problem {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 8px 14px;
		min-height: 52px;
	}
	.ptxt {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		font-size: 14px;
	}
	.ptxt a {
		color: var(--color-text);
		text-decoration: none;
	}
	.ptxt a:hover {
		color: var(--color-accent-300);
	}
	.ptxt .muted {
		font-size: 12px;
	}
	.nodes {
		display: grid;
		gap: var(--gap);
		grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
	}
	.node {
		color: inherit;
		text-decoration: none;
		gap: 10px;
	}
	.node:hover {
		color: inherit;
		background: color-mix(in srgb, var(--color-surface) 92%, var(--color-text));
	}
	.node:hover .faint {
		color: var(--color-neutral-400);
	}
	.cards {
		grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
	}
	@media (min-width: 1280px) {
		.cards {
			grid-template-columns: repeat(4, minmax(0, 1fr));
		}
	}
	.edit ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.edit li {
		display: flex;
		align-items: center;
		gap: 8px;
		min-height: 36px;
	}
</style>
