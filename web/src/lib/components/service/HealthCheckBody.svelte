<script lang="ts">
	// Health-check template: stat tiles, 24 h response chart, uptime and last incident.
	import {
		api,
		type Health,
		type Schemas,
		type ServiceSnapshot,
		type ServiceView
	} from '#lib/api/client.js';
	import { dateTime, duration, percent } from '#lib/format.js';
	import Icon from '../Icon.svelte';
	import StatTile from '../StatTile.svelte';
	import TimeChart from '../TimeChart.svelte';
	import UptimeStrip from '../UptimeStrip.svelte';

	let {
		service,
		snapshot,
		tick
	}: { service: ServiceView; snapshot: ServiceSnapshot | null; tick: number } = $props();

	let history = $state<Schemas['ServiceHistory'] | null>(null);
	$effect(() => {
		void tick;
		void api
			.GET('/services/{id}/history', { params: { path: { id: service.id } } })
			.then((r) => (history = r.data ?? history));
	});

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
	const pctValue = (v: number | null | undefined) =>
		v != null ? percent(v).replace(' %', '') : '–';
</script>

<div class="stats">
	<StatTile
		label="Response"
		value={service.response_ms ?? '–'}
		unit={service.response_ms != null ? 'ms' : ''}
	/>
	<StatTile
		label="Uptime · 24 h"
		value={pctValue(history?.uptime_24h)}
		unit={history?.uptime_24h != null ? '%' : ''}
	/>
	<StatTile
		label="Uptime · 7 d"
		value={pctValue(history?.uptime_7d)}
		unit={history?.uptime_7d != null ? '%' : ''}
	/>
	<StatTile
		label={check?.check === 'tcp' ? 'Check' : 'Last status'}
		value={check?.check === 'tcp' ? 'TCP' : (check?.status_code ?? '–')}
	/>
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

<style>
	.stats {
		display: grid;
		gap: 10px;
		grid-template-columns: repeat(4, minmax(0, 1fr));
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
