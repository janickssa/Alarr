<script lang="ts">
	// Proxmox: cluster overview (quorum, nodes, storage) and the guest list with filters,
	// multi-select bulk start/shutdown and a guest detail panel (?guest=<vmid>).
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import { api, errorMessage, unwrap, type Guest, type Schemas } from '#lib/api/client.js';
	import DetailPanel from '#lib/components/DetailPanel.svelte';
	import Dialog from '#lib/components/Dialog.svelte';
	import EmptyState from '#lib/components/EmptyState.svelte';
	import ErrorState from '#lib/components/ErrorState.svelte';
	import HealthBadge from '#lib/components/HealthBadge.svelte';
	import HealthDot from '#lib/components/HealthDot.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import PowerMenu from '#lib/components/PowerMenu.svelte';
	import GuestDetail from '#lib/components/proxmox/GuestDetail.svelte';
	import NodeCard from '#lib/components/proxmox/NodeCard.svelte';
	import RelativeTime from '#lib/components/RelativeTime.svelte';
	import ResourceBar from '#lib/components/ResourceBar.svelte';
	import SearchField from '#lib/components/SearchField.svelte';
	import SegFilter from '#lib/components/SegFilter.svelte';
	import { duration, guestId, pct, usedOf } from '#lib/format.js';
	import { HEALTH } from '#lib/health.js';
	import { refetchOnLive } from '#lib/live.svelte.js';
	import { power } from '#lib/power.svelte.js';
	import { session } from '#lib/session.svelte.js';

	let cluster = $state<Schemas['ClusterOverview'] | null>(null);
	let guests = $state<Guest[]>([]);
	let error = $state('');
	let cpu = $state<Record<string, (number | null)[]>>({});

	let query = $state('');
	let status = $state<'all' | 'running' | 'stopped'>('all');
	let nodeFilter = $state<string | null>(null);
	let typeFilter = $state<'lxc' | 'qemu' | null>(null);
	let selected = $state<number[]>([]);
	let bulkOpen = $state(false);

	const pveId = $derived(session.proxmox?.id ?? '');
	const canAct = $derived(pveId ? session.canAct(pveId) : false);

	async function load() {
		try {
			const [c, g] = await Promise.all([api.GET('/proxmox/cluster'), api.GET('/proxmox/guests')]);
			cluster = unwrap(c);
			guests = unwrap(g);
			error = '';
		} catch (e) {
			error = errorMessage(e);
		}
	}

	async function loadCpu() {
		for (const n of cluster?.nodes.filter((n) => n.online) ?? []) {
			const res = await api.GET('/proxmox/nodes/{node}/rrd', {
				params: { path: { node: n.name }, query: { range: 'hour' } }
			});
			if (res.data) cpu[n.name] = res.data.map((p) => p.cpu ?? null);
		}
	}

	onMount(() => {
		void load().then(loadCpu);
		const t = setInterval(loadCpu, 60_000);
		return () => clearInterval(t);
	});
	refetchOnLive(() => (pveId ? [pveId] : []), load, { minInterval: 2500 });

	const nodes = $derived([...new Set(guests.map((g) => g.node))].sort());
	const visible = $derived(guests.filter((g) => !g.template));
	const counts = $derived({
		all: visible.length,
		running: visible.filter((g) => g.status === 'running').length,
		stopped: visible.filter((g) => g.status !== 'running').length
	});
	const shown = $derived(
		visible.filter(
			(g) =>
				(status === 'all' ||
					(status === 'running' ? g.status === 'running' : g.status !== 'running')) &&
				(!nodeFilter || g.node === nodeFilter) &&
				(!typeFilter || g.guest_type === typeFilter) &&
				(!query ||
					`${g.name} ${g.vmid} ${g.tags.join(' ')} ${g.linked_services.join(' ')}`
						.toLowerCase()
						.includes(query.toLowerCase()))
		)
	);
	const selectedGuests = $derived(guests.filter((g) => selected.includes(g.vmid)));
	const openVmid = $derived(Number(page.url.searchParams.get('guest')) || null);
	const openGuest = $derived(guests.find((g) => g.vmid === openVmid) ?? null);
	const serviceName = (id: string) => session.services.find((s) => s.id === id)?.name ?? id;

	function openGuestPanel(vmid: number | null) {
		const url = vmid ? `${resolve('/proxmox')}?guest=${vmid}` : resolve('/proxmox');
		void goto(url, { reset: false, replace: openVmid != null && vmid != null });
	}

	function toggle(vmid: number) {
		selected = selected.includes(vmid) ? selected.filter((v) => v !== vmid) : [...selected, vmid];
	}

	const allShownSelected = $derived(
		shown.length > 0 && shown.every((g) => selected.includes(g.vmid))
	);
	function toggleAll() {
		const ids = shown.map((g) => g.vmid);
		selected = allShownSelected
			? selected.filter((v) => !ids.includes(v))
			: [...new Set([...selected, ...ids])];
	}

	async function bulkStart() {
		for (const g of selectedGuests.filter((g) => g.status !== 'running'))
			await power.send(g, 'start');
		selected = [];
	}

	async function bulkShutdown() {
		bulkOpen = false;
		for (const g of selectedGuests.filter((g) => g.status === 'running'))
			await power.send(g, 'shutdown');
		selected = [];
	}

	function statusHealth(g: Guest) {
		return g.status === 'running'
			? 'healthy'
			: g.status === 'paused'
				? 'warning'
				: g.status === 'stopped'
					? 'disabled'
					: 'unknown';
	}
</script>

<svelte:head><title>Proxmox</title></svelte:head>

<div class="layout">
	<div class="page">
		{#if error && !cluster}
			<ErrorState title="Can't reach Proxmox" detail={error} onretry={load} />
		{:else if !cluster}
			<div class="skeleton" style="height:40px;width:300px"></div>
			<div class="grid">
				{#each Array(3), i (i)}<div class="skeleton" style="height:220px"></div>{/each}
			</div>
		{:else}
			<div class="page-header">
				<h1 class="page-title">{cluster.cluster.name ?? 'Proxmox'}</h1>
				<HealthBadge
					health={cluster.cluster.quorate ? 'healthy' : 'down'}
					label={cluster.cluster.quorate ? 'Quorate' : 'No quorum'}
				/>
				{#if cluster.health !== 'healthy'}<HealthBadge
						health={cluster.health}
						label={cluster.health_reason ?? HEALTH[cluster.health].label}
					/>{/if}
				<span class="spacer"></span>
				<span class="caption faint">
					{#if cluster.cluster.pve_version}<span class="mono"
							>PVE {cluster.cluster.pve_version}</span
						> ·{/if}
					<RelativeTime value={cluster.updated_at} prefix="Updated " />
				</span>
			</div>

			<section aria-label="Nodes" class="nodes">
				{#each cluster.nodes as n (n.name)}
					<NodeCard node={n} cpuHistory={cpu[n.name] ?? []} />
				{/each}
			</section>

			<section class="card" aria-labelledby="storage-h">
				<h2 id="storage-h" class="section-title">Storage</h2>
				<table class="table">
					<thead
						><tr
							><th>Pool</th><th class="hide-sm">Type</th><th class="hide-sm" style="width:40%"
								>Usage</th
							><th class="right">Used</th></tr
						></thead
					>
					<tbody>
						{#each cluster.storage.filter((s) => !s.shared || cluster?.storage.find((x) => x.storage === s.storage) === s) as s (s.id)}
							<tr>
								<td class="mono"
									>{s.shared ? s.storage : s.id}{#if s.shared}<span class="tag tag-neutral shared"
											>shared</span
										>{/if}</td
								>
								<td class="muted hide-sm"
									>{s.plugin_type ?? ''}{#if !s.active}
										· <span style="color:var(--health-down)">inactive</span>{/if}</td
								>
								<td class="hide-sm"
									><ResourceBar fraction={s.total ? (s.used ?? 0) / s.total : null} compact /></td
								>
								<td class="right mono num"
									>{usedOf(s.used, s.total)} · {pct(s.total ? (s.used ?? 0) / s.total : null)}</td
								>
							</tr>
						{/each}
					</tbody>
				</table>
			</section>

			<section aria-labelledby="guests-h" class="guests">
				<div class="row wrap">
					<h2 id="guests-h" class="section-title">Guests</h2>
					<span class="spacer"></span>
					<SearchField
						bind:value={query}
						placeholder="Name, ID, tag or service"
						label="Filter guests"
						width="260px"
					/>
					<SegFilter
						label="Status"
						bind:value={status}
						options={[
							{ value: 'all', label: 'All', count: counts.all },
							{ value: 'running', label: 'Running', count: counts.running },
							{ value: 'stopped', label: 'Stopped', count: counts.stopped }
						]}
					/>
				</div>
				<div class="row wrap chips">
					{#each nodes as n (n)}
						<button
							class="tag {nodeFilter === n ? 'tag-outline' : 'tag-neutral'}"
							aria-pressed={nodeFilter === n}
							onclick={() => (nodeFilter = nodeFilter === n ? null : n)}>Node: {n}</button
						>
					{/each}
					<button
						class="tag {typeFilter === 'lxc' ? 'tag-outline' : 'tag-neutral'}"
						aria-pressed={typeFilter === 'lxc'}
						onclick={() => (typeFilter = typeFilter === 'lxc' ? null : 'lxc')}>Type: CT</button
					>
					<button
						class="tag {typeFilter === 'qemu' ? 'tag-outline' : 'tag-neutral'}"
						aria-pressed={typeFilter === 'qemu'}
						onclick={() => (typeFilter = typeFilter === 'qemu' ? null : 'qemu')}>Type: VM</button
					>
					{#if nodeFilter || typeFilter || query || status !== 'all'}
						<button
							class="btn btn-ghost btn-sm"
							onclick={() => {
								nodeFilter = null;
								typeFilter = null;
								query = '';
								status = 'all';
							}}>Clear</button
						>
					{/if}
				</div>

				{#if selected.length > 0}
					<div class="bulk" role="region" aria-label="Bulk actions">
						<span>{selected.length} selected</span>
						<button
							class="btn btn-secondary btn-sm"
							disabled={!canAct || !selectedGuests.some((g) => g.status !== 'running')}
							onclick={bulkStart}><Icon name="ph-play" />Start</button
						>
						<button
							class="btn btn-secondary btn-sm"
							disabled={!canAct || !selectedGuests.some((g) => g.status === 'running')}
							onclick={() => (bulkOpen = true)}><Icon name="ph-power" />Shutdown</button
						>
						<button class="btn btn-ghost btn-sm" onclick={() => (selected = [])}
							>Clear selection</button
						>
					</div>
				{/if}

				{#if shown.length === 0}
					<EmptyState title="No guests match" detail="Change or clear the filters." />
				{:else}
					<div class="card table-card">
						<table class="table" class:narrow={openGuest != null}>
							<thead>
								<tr>
									<th class="hide-sm" style="width:28px"
										><input
											type="checkbox"
											aria-label="Select all shown guests"
											checked={allShownSelected}
											onchange={toggleAll}
										/></th
									>
									<th>Status</th>
									<th class="hide-sm">ID</th>
									<th>Name</th>
									<th class="hide-sm">Node</th>
									<th class="right">CPU</th>
									<th class="wide" style="width:16%">RAM</th>
									<th class="right wide">Uptime</th>
									<th class="wide">Services</th>
									<th><span class="visually-hidden">Power</span></th>
								</tr>
							</thead>
							<tbody>
								{#each shown as g (g.vmid)}
									<tr
										aria-selected={openVmid === g.vmid}
										class="clickable"
										onclick={(e) => {
											if (!(e.target as HTMLElement).closest('button, input, a'))
												openGuestPanel(g.vmid);
										}}
									>
										<td class="hide-sm"
											><input
												type="checkbox"
												aria-label="Select {g.name}"
												checked={selected.includes(g.vmid)}
												onchange={() => toggle(g.vmid)}
											/></td
										>
										<td
											><span class="hide-sm"
												><HealthDot
													health={statusHealth(g)}
													label={g.status[0].toUpperCase() + g.status.slice(1)}
												/></span
											><span class="show-sm"
												><HealthDot
													health={statusHealth(g)}
													label={g.status === 'running' ? '' : g.status}
												/></span
											></td
										>
										<td class="mono hide-sm">{guestId(g.guest_type, g.vmid)}</td>
										<td>
											<button class="link" onclick={() => openGuestPanel(g.vmid)}>{g.name}</button>
											{#if g.lock}<span class="tag tag-neutral">{g.lock}</span>{/if}
											<span class="show-sm caption faint mono"
												>{guestId(g.guest_type, g.vmid)} · {g.node}</span
											>
										</td>
										<td class="mono muted hide-sm">{g.node}</td>
										<td class="right mono num">{g.status === 'running' ? pct(g.cpu_usage) : '–'}</td
										>
										<td class="wide"
											>{#if g.status === 'running'}<ResourceBar
													fraction={g.mem_total ? (g.mem_used ?? 0) / g.mem_total : null}
													value={usedOf(g.mem_used, g.mem_total)}
													compact
												/>{/if}</td
										>
										<td class="right num wide"
											>{g.status === 'running' ? duration(g.uptime_s) : '–'}</td
										>
										<td class="svcs wide">
											{#each g.linked_services as id (id)}
												<a class="tag tag-accent" href={resolve('/services/[id]', { id })}
													>{serviceName(id)}</a
												>
											{/each}
										</td>
										<td class="right"
											><PowerMenu
												guest={g}
												disabled={!canAct}
												linked={g.linked_services.map(serviceName)}
											/></td
										>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				{/if}
			</section>
		{/if}
	</div>

	{#if openGuest && pveId}
		<DetailPanel
			title="{openGuest.name} details"
			showTitle={false}
			onclose={() => openGuestPanel(null)}
		>
			<GuestDetail guest={openGuest} {pveId} />
		</DetailPanel>
	{/if}
</div>

<Dialog
	bind:open={bulkOpen}
	title="Shut down {selectedGuests.filter((g) => g.status === 'running').length} guests?"
>
	<div class="dlg-title">
		Shut down {selectedGuests.filter((g) => g.status === 'running').length} guests?
	</div>
	<ul class="dlg-list mono">
		{#each selectedGuests.filter((g) => g.status === 'running') as g (g.vmid)}
			<li>{guestId(g.guest_type, g.vmid)} · {g.name} · {g.node}</li>
		{/each}
	</ul>
	<p class="muted caption">Each guest shuts down cleanly and stays off until you start it.</p>
	{#snippet actions()}
		<button class="btn btn-secondary" onclick={() => (bulkOpen = false)}>Cancel</button>
		<button class="btn btn-primary" onclick={bulkShutdown}><Icon name="ph-power" />Shutdown</button>
	{/snippet}
</Dialog>

<style>
	.layout {
		display: flex;
		align-items: flex-start;
	}
	.layout > .page {
		flex: 1;
		min-width: 0;
	}
	.nodes {
		display: grid;
		gap: var(--gap);
		grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
	}
	.shared {
		margin-left: 6px;
	}
	.guests {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}
	.wrap {
		flex-wrap: wrap;
	}
	.chips {
		gap: 6px;
	}
	.bulk {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 12px;
		border-radius: var(--radius-md);
		background: color-mix(in srgb, var(--color-accent) 10%, var(--color-surface));
		font-size: 13px;
	}
	.table-card {
		padding: 4px 8px;
		overflow-x: auto;
	}
	.narrow :global(.wide) {
		display: none;
	}
	@media (max-width: 767px) {
		.table-card :global(.wide) {
			display: none;
		}
		.show-sm.caption {
			display: block;
		}
		.link {
			font-size: 15px;
			min-height: 24px;
		}
	}
	.clickable {
		cursor: pointer;
	}
	.link {
		background: none;
		border: 0;
		padding: 0;
		cursor: pointer;
		font-size: 13px;
		text-align: left;
	}
	.link:hover {
		color: var(--color-accent-300);
	}
	.svcs a {
		text-decoration: none;
		margin-right: 4px;
	}
	.dlg-title {
		font-size: 18px;
		font-weight: 500;
	}
	.dlg-list {
		margin: 0;
		padding: 8px 10px 8px 26px;
		border-radius: var(--radius-md);
		background: var(--color-bg);
		font-size: 12.5px;
		max-height: 200px;
		overflow: auto;
	}
</style>
