<script lang="ts">
	// Guest detail (right-side panel): status, power, graphs, info, linked services,
	// last backup and recent tasks.
	import { resolve } from '$app/paths';
	import {
		api,
		errorMessage,
		unwrap,
		type Guest,
		type RrdPoint,
		type RrdRange,
		type Schemas
	} from '#lib/api/client.js';
	import { bytes, dateTime, duration, guestId, pct, rate, usedOf } from '#lib/format.js';
	import { HEALTH } from '#lib/health.js';
	import { session } from '#lib/session.svelte.js';
	import ErrorState from '../ErrorState.svelte';
	import HealthBadge from '../HealthBadge.svelte';
	import Icon from '../Icon.svelte';
	import PowerMenu from '../PowerMenu.svelte';
	import RelativeTime from '../RelativeTime.svelte';
	import TimeChart from '../TimeChart.svelte';

	let { guest, pveId }: { guest: Guest; pveId: string } = $props();

	let detail = $state<Schemas['GuestDetail'] | null>(null);
	let error = $state('');
	let range = $state<RrdRange>('hour');
	let rrd = $state<RrdPoint[]>([]);
	let rrdLoading = $state(false);

	async function loadDetail(vmid: number) {
		try {
			detail = unwrap(await api.GET('/proxmox/guests/{vmid}', { params: { path: { vmid } } }));
			error = '';
		} catch (e) {
			error = errorMessage(e);
		}
	}

	async function loadRrd(vmid: number, r: RrdRange) {
		rrdLoading = true;
		try {
			rrd = unwrap(
				await api.GET('/proxmox/guests/{vmid}/rrd', {
					params: { path: { vmid }, query: { range: r } }
				})
			);
		} catch {
			rrd = [];
		} finally {
			rrdLoading = false;
		}
	}

	// Reload when the guest changes or its status changes (after a power action).
	$effect(() => {
		void guest.status;
		void loadDetail(guest.vmid);
	});
	$effect(() => {
		void loadRrd(guest.vmid, range);
	});

	const times = $derived(rrd.map((p) => new Date(p.time).getTime() / 1000));
	const linkedNames = $derived(
		(detail?.linked_services ?? []).map(
			(s) => `${s.name} (${HEALTH[s.health].label.toLowerCase()})`
		)
	);
	const status = $derived(
		guest.status === 'running'
			? 'healthy'
			: guest.status === 'paused'
				? 'warning'
				: guest.status === 'stopped'
					? 'disabled'
					: 'unknown'
	);
</script>

<div class="head">
	<div class="title-row">
		<h2 class="title">{guest.name}</h2>
		<HealthBadge
			health={status}
			label={guest.status[0].toUpperCase() + guest.status.slice(1)}
			small
		/>
		<span class="spacer"></span>
		<PowerMenu {guest} linked={linkedNames} disabled={!session.canAct(pveId)} />
	</div>
	<span class="mono muted">{guestId(guest.guest_type, guest.vmid)} · {guest.node}</span>
</div>

<TimeChart
	title="CPU"
	bind:range
	loading={rrdLoading}
	{times}
	yMax={1}
	series={[{ label: 'CPU', values: rrd.map((p) => p.cpu ?? null), format: (v) => pct(v) }]}
/>
<TimeChart
	title="Memory"
	ranges={false}
	{range}
	loading={rrdLoading}
	{times}
	series={[{ label: 'Used', values: rrd.map((p) => p.mem_used ?? null), format: (v) => bytes(v) }]}
/>
<TimeChart
	title="Network"
	ranges={false}
	{range}
	loading={rrdLoading}
	{times}
	series={[
		{ label: 'In', values: rrd.map((p) => p.net_in ?? null), format: (v) => rate(v) },
		{ label: 'Out', values: rrd.map((p) => p.net_out ?? null), format: (v) => rate(v) }
	]}
/>

<section class="card">
	<h3 class="overline">Info</h3>
	<dl class="facts">
		<dt>Node</dt>
		<dd class="mono">{guest.node}</dd>
		<dt>Type</dt>
		<dd>{guest.guest_type === 'lxc' ? 'Container (LXC)' : 'Virtual machine (QEMU)'}</dd>
		<dt>CPU</dt>
		<dd>{guest.cpu_count ?? '–'} cores · {pct(guest.cpu_usage)}</dd>
		<dt>Memory</dt>
		<dd>{usedOf(guest.mem_used, guest.mem_total)}</dd>
		{#if guest.disk_total}<dt>Disk</dt>
			<dd>
				{guest.disk_used ? usedOf(guest.disk_used, guest.disk_total) : bytes(guest.disk_total)}
			</dd>{/if}
		<dt>Uptime</dt>
		<dd>{guest.status === 'running' ? duration(guest.uptime_s) : '–'}</dd>
		{#if detail}<dt>HA</dt>
			<dd>{detail.ha_managed ? (guest.ha_state ?? 'managed') : 'Not managed'}</dd>{/if}
		{#if guest.lock}<dt>Lock</dt>
			<dd class="mono">{guest.lock}</dd>{/if}
		{#if guest.tags.length}
			<dt>Tags</dt>
			<dd class="tags">
				{#each guest.tags as t (t)}<span class="tag tag-neutral">{t}</span>{/each}
			</dd>
		{/if}
	</dl>
</section>

{#if error}
	<ErrorState
		title="Can't load guest details"
		detail={error}
		onretry={() => loadDetail(guest.vmid)}
	/>
{:else if detail}
	<section class="card">
		<h3 class="overline">Linked services</h3>
		{#if detail.linked_services.length === 0}
			<p class="muted caption">
				None. Link one in the service settings or tag the guest <span class="mono"
					>svc-&lt;service id&gt;</span
				>.
			</p>
		{:else}
			<ul class="linked">
				{#each detail.linked_services as s (s.id)}
					<li>
						<a href={resolve('/services/[id]', { id: s.id })}>{s.name}</a>
						<HealthBadge health={s.health} small />
					</li>
				{/each}
			</ul>
		{/if}
	</section>

	<section class="card">
		<h3 class="overline">Last backup</h3>
		{#if detail.last_backup}
			<div class="row caption">
				<Icon
					name={detail.last_backup.state === 'ok' ? 'ph-check-circle' : 'ph-x-circle'}
					fill
					color={detail.last_backup.state === 'ok' ? 'var(--health-healthy)' : 'var(--health-down)'}
				/>
				<span title={dateTime(detail.last_backup.started_at)}
					><RelativeTime value={detail.last_backup.started_at} /></span
				>
				<span class="faint">{detail.last_backup.exit_status ?? ''}</span>
			</div>
		{:else}
			<p class="muted caption">No backup task in the recent history.</p>
		{/if}
	</section>

	<section class="card">
		<h3 class="overline">Recent tasks</h3>
		{#if detail.recent_tasks.length === 0}
			<p class="muted caption">No recent tasks.</p>
		{:else}
			<table class="table">
				<tbody>
					{#each detail.recent_tasks as t (t.upid)}
						<tr>
							<td class="mono">{t.kind ?? 'task'}</td>
							<td
								><HealthBadge
									health={t.state === 'ok'
										? 'healthy'
										: t.state === 'running'
											? 'in_progress'
											: 'down'}
									label={t.state === 'ok' ? 'OK' : t.state === 'running' ? 'Running' : 'Failed'}
									small
								/></td
							>
							<td class="caption"><RelativeTime value={t.started_at} /></td>
							<td class="caption faint">{t.user ?? ''}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		{/if}
	</section>
{/if}

<style>
	.head {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.title-row {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.title {
		font: var(--text-title);
	}
	.facts {
		display: grid;
		grid-template-columns: 90px 1fr;
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
	.tags {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
	}
	.linked {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.linked li {
		display: flex;
		align-items: center;
		justify-content: space-between;
		font-size: 13px;
	}
</style>
