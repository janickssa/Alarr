<script lang="ts">
	// Service page (health-check template): header with linked container and power menu,
	// stat tiles, actions, 24 h response chart, uptime and last incident.
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import {
		api,
		errorMessage,
		unwrap,
		type Guest,
		type Health,
		type Schemas,
		type ServiceSnapshot,
		type ServiceView
	} from '#lib/api/client.js';
	import ErrorState from '#lib/components/ErrorState.svelte';
	import HealthBadge from '#lib/components/HealthBadge.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import PowerMenu from '#lib/components/PowerMenu.svelte';
	import RelativeTime from '#lib/components/RelativeTime.svelte';
	import ServiceIcon from '#lib/components/ServiceIcon.svelte';
	import SkeletonCard from '#lib/components/SkeletonCard.svelte';
	import StatTile from '#lib/components/StatTile.svelte';
	import TimeChart from '#lib/components/TimeChart.svelte';
	import UptimeStrip from '#lib/components/UptimeStrip.svelte';
	import { dateTime, duration, guestId, percent } from '#lib/format.js';
	import { refetchOnLive } from '#lib/live.svelte.js';
	import { session } from '#lib/session.svelte.js';
	import { toasts } from '#lib/toasts.svelte.js';

	const id = $derived(page.params.id ?? '');
	let service = $state<ServiceView | null>(null);
	let snapshot = $state<ServiceSnapshot | null>(null);
	let history = $state<Schemas['ServiceHistory'] | null>(null);
	let guest = $state<Guest | null>(null);
	let error = $state('');

	async function load(sid: string) {
		try {
			const s = unwrap(await api.GET('/services/{id}', { params: { path: { id: sid } } }));
			if (s.kind === 'proxmox') {
				await goto(resolve('/proxmox'), { replace: true });
				return;
			}
			service = s;
			const [snap, hist] = await Promise.all([
				api.GET('/services/{id}/snapshot', { params: { path: { id: sid } } }),
				api.GET('/services/{id}/history', { params: { path: { id: sid } } })
			]);
			snapshot = snap.data ?? null;
			history = hist.data ?? null;
			error = '';
			if (s.linked_guest && session.proxmox) {
				const g = await api.GET('/proxmox/guests/{vmid}', {
					params: { path: { vmid: s.linked_guest.vmid } }
				});
				guest = g.data?.guest ?? null;
			} else guest = null;
		} catch (e) {
			error = errorMessage(e);
		}
	}

	$effect(() => {
		service = null;
		void load(id);
	});
	refetchOnLive(
		() => [id, ...(session.proxmox ? [session.proxmox.id] : [])],
		() => load(id),
		{ minInterval: 3000 }
	);

	const check = $derived(snapshot?.data.type === 'health_check' ? snapshot.data.value : null);
	const times = $derived((history?.points ?? []).map((p) => new Date(p.at).getTime() / 1000));
	const buckets = $derived.by(() => {
		const now = Math.floor(Date.now() / 3_600_000) * 3600;
		const rank: Record<string, number> = { healthy: 2, warning: 1, down: 0 };
		return Array.from({ length: 24 }, (_, i) => {
			const start = now - (23 - i) * 3600;
			const pts = (history?.points ?? []).filter((p) => {
				const t = new Date(p.at).getTime() / 1000;
				return t >= start && t < start + 3600;
			});
			const worst = pts.reduce<Health | null>(
				(w, p) => (w == null || rank[p.health] < rank[w] ? p.health : w),
				null
			);
			return {
				start: new Date(start * 1000).toISOString(),
				health: worst ?? ('unknown' as Health)
			};
		});
	});

	async function refresh() {
		await api.POST('/services/{id}/actions/{action}', {
			params: { path: { id, action: 'refresh' } },
			body: { confirm: false }
		});
		toasts.push({ kind: 'info', title: `Checking ${service?.name} now` });
	}
</script>

<svelte:head><title>{service?.name ?? 'Service'}</title></svelte:head>

<div class="page">
	{#if error && !service}
		<ErrorState title="Can't load this service" detail={error} onretry={() => load(id)} />
	{:else if !service}
		<div class="skeleton" style="height:44px;width:280px"></div>
		<div class="grid">
			{#each Array(4), i (i)}<SkeletonCard />{/each}
		</div>
	{:else}
		<header class="svc-head">
			<ServiceIcon
				icon={service.icon}
				hue={service.accent_hue}
				chroma={service.accent_chroma}
				size={44}
			/>
			<div class="titles">
				<div class="row">
					<h1 class="page-title">{service.name}</h1>
					<HealthBadge health={service.health} />
				</div>
				<span class="caption muted">
					{#if service.health_reason}{service.health_reason} ·
					{/if}
					{#if service.updated_at}<RelativeTime value={service.updated_at} prefix="Checked " />{/if}
				</span>
			</div>
			<span class="spacer"></span>
			{#if service.linked_guest}
				<div class="guest-chip">
					<Icon name="ph-cube" color="var(--color-neutral-400)" />
					<span class="mono"
						>{guestId(service.linked_guest.guest_type, service.linked_guest.vmid)} · {service
							.linked_guest.name} · {service.linked_guest.node}</span
					>
					<HealthBadge
						health={service.linked_guest.status === 'running' ? 'healthy' : 'disabled'}
						label={service.linked_guest.status}
						small
					/>
					{#if guest && session.proxmox}
						<PowerMenu
							{guest}
							linked={[service.name]}
							disabled={!session.canAct(session.proxmox.id)}
						/>
					{/if}
				</div>
			{/if}
			{#if service.base_url}
				<a
					class="btn btn-primary"
					href={service.base_url}
					target="_blank"
					rel="noreferrer noopener"
				>
					<Icon name="ph-arrow-square-out" />Open web UI
				</a>
			{/if}
		</header>

		<div class="stats">
			<StatTile
				label="Response"
				value={service.response_ms ?? '–'}
				unit={service.response_ms != null ? 'ms' : ''}
			/>
			<StatTile
				label="Uptime · 24 h"
				value={history?.uptime_24h != null ? percent(history.uptime_24h).replace(' %', '') : '–'}
				unit={history?.uptime_24h != null ? '%' : ''}
			/>
			<StatTile
				label="Uptime · 7 d"
				value={history?.uptime_7d != null ? percent(history.uptime_7d).replace(' %', '') : '–'}
				unit={history?.uptime_7d != null ? '%' : ''}
			/>
			<StatTile
				label={check?.check === 'tcp' ? 'Check' : 'Last status'}
				value={check?.check === 'tcp' ? 'TCP' : (check?.status_code ?? '–')}
			/>
		</div>

		<div class="row actions">
			{#if session.canAct(service.id)}
				<button class="btn btn-secondary" onclick={refresh}
					><Icon name="ph-arrow-clockwise" />Check now</button
				>
			{/if}
			{#if session.isOwner}
				<a class="btn btn-secondary" href={resolve('/settings/services/[id]', { id: service.id })}
					><Icon name="ph-gear-six" />Settings</a
				>
			{/if}
		</div>

		<div class="two">
			<TimeChart
				title="Response time · 24 h"
				ranges={false}
				range="day"
				{times}
				loading={!history}
				series={[
					{
						label: 'Response',
						values: (history?.points ?? []).map((p) =>
							p.health === 'down' ? null : (p.response_ms ?? null)
						),
						format: (v) => `${Math.round(v)} ms`
					}
				]}
			/>
			<div class="side">
				<section class="card">
					<h2 class="overline">Uptime · 24 h</h2>
					<UptimeStrip {buckets} uptime={history?.uptime_24h} />
				</section>
				<section class="card">
					<h2 class="overline">Last incident</h2>
					{#if history?.last_incident}
						{@const inc = history.last_incident}
						<p class="incident">
							<Icon name="ph-x-circle" fill color="var(--health-down)" />
							{inc.end
								? `Down for ${duration(inc.minutes * 60)}`
								: `Down for ${duration(inc.minutes * 60)} and counting`}
						</p>
						<span class="caption muted"
							>{dateTime(inc.start)}{inc.end ? ` – ${dateTime(inc.end)}` : ''}</span
						>
					{:else}
						<p class="caption muted">No downtime in the last 7 days.</p>
					{/if}
				</section>
				<section class="card">
					<h2 class="overline">Check</h2>
					<dl class="facts">
						<dt>Type</dt>
						<dd>{check?.check === 'tcp' ? 'TCP connect' : 'HTTP(S) GET'}</dd>
						<dt>Target</dt>
						<dd class="mono target">{check?.target ?? service.base_url ?? '–'}</dd>
					</dl>
				</section>
			</div>
		</div>
	{/if}
</div>

<style>
	.svc-head {
		display: flex;
		align-items: center;
		gap: 14px;
		flex-wrap: wrap;
	}
	.titles {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.titles .row {
		gap: 12px;
	}
	.guest-chip {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 4px 4px 4px 10px;
		border-radius: var(--radius-md);
		background: var(--color-surface);
		font-size: 12.5px;
	}
	.stats {
		display: grid;
		gap: 10px;
		grid-template-columns: repeat(4, minmax(0, 1fr));
	}
	.actions {
		gap: 8px;
	}
	.two {
		display: grid;
		gap: var(--gap);
		grid-template-columns: minmax(0, 2fr) minmax(260px, 1fr);
		align-items: start;
	}
	.side {
		display: flex;
		flex-direction: column;
		gap: var(--gap);
	}
	.incident {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 14px;
	}
	.facts {
		display: grid;
		grid-template-columns: 60px 1fr;
		gap: 6px 12px;
		margin: 0;
		font-size: 13px;
	}
	dt {
		color: var(--color-neutral-400);
	}
	dd {
		margin: 0;
	}
	.target {
		word-break: break-all;
		font-size: 12px;
	}
	@media (max-width: 1000px) {
		.two {
			grid-template-columns: 1fr;
		}
		.stats {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
	}
</style>
