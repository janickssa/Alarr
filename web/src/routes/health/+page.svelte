<script lang="ts">
	// Health: every service and guest with status, response time, uptime and a 24 h timeline.
	import { resolve } from '$app/paths';
	import { onMount } from 'svelte';
	import { api, errorMessage, unwrap, type HealthReport } from '#lib/api/client.js';
	import EmptyState from '#lib/components/EmptyState.svelte';
	import ErrorState from '#lib/components/ErrorState.svelte';
	import HealthDot from '#lib/components/HealthDot.svelte';
	import RelativeTime from '#lib/components/RelativeTime.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import SearchField from '#lib/components/SearchField.svelte';
	import SegFilter from '#lib/components/SegFilter.svelte';
	import UptimeStrip from '#lib/components/UptimeStrip.svelte';
	import { dateTime, guestId, ms, percent } from '#lib/format.js';
	import { refetchOnLive } from '#lib/live.svelte.js';
	import { session } from '#lib/session.svelte.js';

	let report = $state<HealthReport | null>(null);
	let error = $state('');
	let filter = $state<'all' | 'problems' | 'services' | 'guests'>('all');
	let query = $state('');

	async function load() {
		try {
			report = unwrap(await api.GET('/health'));
			error = '';
		} catch (e) {
			error = errorMessage(e);
		}
	}
	onMount(load);
	refetchOnLive(() => session.services.map((s) => s.id), load, { health: true, minInterval: 5000 });

	const entries = $derived(report?.entries ?? []);
	const isProblem = (h: string) => h === 'down' || h === 'warning';
	const counts = $derived({
		all: entries.length,
		problems: entries.filter((e) => isProblem(e.health) || e.warnings.length > 0).length,
		services: entries.filter((e) => e.kind === 'service').length,
		guests: entries.filter((e) => e.kind === 'guest').length
	});
	const shown = $derived(
		entries
			.filter((e) =>
				filter === 'all'
					? true
					: filter === 'problems'
						? isProblem(e.health) || e.warnings.length > 0
						: filter === 'services'
							? e.kind === 'service'
							: e.kind === 'guest'
			)
			.filter(
				(e) => !query || `${e.name} ${e.vmid ?? ''}`.toLowerCase().includes(query.toLowerCase())
			)
			.sort((a, b) => Number(isProblem(b.health)) - Number(isProblem(a.health)))
	);
</script>

<svelte:head><title>Health</title></svelte:head>

<div class="page">
	<div class="page-header">
		<h1 class="page-title">Health</h1>
	</div>
	<div class="row wrap">
		<SearchField
			bind:value={query}
			placeholder="Filter by name or ID"
			label="Filter health entries"
		/>
		<SegFilter
			label="Show"
			bind:value={filter}
			options={[
				{ value: 'all', label: 'All', count: counts.all },
				{ value: 'problems', label: 'Problems', count: counts.problems },
				{ value: 'services', label: 'Services', count: counts.services },
				{ value: 'guests', label: 'Guests', count: counts.guests }
			]}
		/>
	</div>

	{#if error && !report}
		<ErrorState title="Can't load health" detail={error} onretry={load} />
	{:else if !report}
		{#each Array(8), i (i)}<div class="skeleton" style="height:36px"></div>{/each}
	{:else if shown.length === 0}
		<EmptyState
			icon={filter === 'problems' ? 'ph-check-circle' : 'ph-tray'}
			title={filter === 'problems' ? 'No problems' : 'Nothing to show'}
			detail={filter === 'problems' ? 'Everything is up and within limits.' : undefined}
		/>
	{:else}
		<div class="card table-card">
			<table class="table">
				<thead>
					<tr>
						<th>Name</th>
						<th>Status</th>
						<th class="right">Response</th>
						<th class="right">24 h</th>
						<th class="right">7 d</th>
						<th style="width:28%">Last 24 h</th>
						<th>Last backup</th>
						<th>Notes</th>
					</tr>
				</thead>
				<tbody>
					{#each shown as e (e.id)}
						<tr>
							<td>
								{#if e.kind === 'service' && e.service_id}
									<a class="name" href={resolve('/services/[id]', { id: e.service_id })}>{e.name}</a
									>
								{:else if e.vmid != null}
									<a class="name" href="{resolve('/proxmox')}?guest={e.vmid}">
										<Icon name="ph-cube" color="var(--color-neutral-500)" />{e.name}
										<span class="mono faint">{guestId(e.guest_type ?? 'lxc', e.vmid)}</span>
									</a>
								{/if}
							</td>
							<td>
								<HealthDot
									health={e.health}
									label={e.kind === 'guest'
										? e.health === 'healthy'
											? 'Running'
											: e.health_reason
												? e.health_reason[0].toUpperCase() + e.health_reason.slice(1)
												: undefined
										: undefined}
								/>
							</td>
							<td class="right mono num">{e.kind === 'service' ? ms(e.response_ms) : ''}</td>
							<td class="right mono num">{percent(e.uptime_24h)}</td>
							<td class="right mono num">{percent(e.uptime_7d)}</td>
							<td><UptimeStrip buckets={e.timeline} height={14} labels={false} /></td>
							<td class="caption nowrap">
								{#if e.backup}
									<span
										class="backup"
										title={e.backup.ok ? dateTime(e.backup.at) : (e.backup.error ?? 'failed')}
										><Icon
											name={e.backup.ok ? 'ph-check-circle' : 'ph-x-circle'}
											fill
											color={e.backup.ok ? 'var(--health-healthy)' : 'var(--health-down)'}
										/><RelativeTime value={e.backup.at} /></span
									>
								{:else if e.kind === 'guest'}<span class="faint">None in 35 d</span>{/if}
							</td>
							<td class="notes">
								{#if e.kind === 'service' && e.health_reason}<span class="reason"
										>{e.health_reason}</span
									>{/if}
								{#each e.warnings as w (w)}
									<span class="warn"
										><Icon name="ph-warning" fill color="var(--health-warning)" />{w}</span
									>
								{/each}
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</div>

<style>
	.wrap {
		flex-wrap: wrap;
		gap: 12px;
	}
	.table-card {
		padding: 4px 8px;
		overflow-x: auto;
	}
	.name {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		color: var(--color-text);
		text-decoration: none;
		white-space: nowrap;
	}
	.name:hover {
		color: var(--color-accent-300);
	}
	.nowrap {
		white-space: nowrap;
	}
	.backup {
		display: inline-flex;
		align-items: center;
		gap: 4px;
	}
	.notes {
		font-size: 12px;
		color: var(--color-neutral-400);
	}
	.warn {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		margin-right: 8px;
	}
	.reason {
		margin-right: 8px;
	}
</style>
