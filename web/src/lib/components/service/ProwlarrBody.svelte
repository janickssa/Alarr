<script lang="ts">
	// Prowlarr: indexers with status, response time and failures in the last 24 h.
	import type { ProwlarrData, ServiceView } from '#lib/api/client.js';
	import { runAction } from '#lib/actions.svelte.js';
	import { ms, time } from '#lib/format.js';
	import { session } from '#lib/session.svelte.js';
	import Banner from '../Banner.svelte';
	import HealthDot from '../HealthDot.svelte';
	import Icon from '../Icon.svelte';
	import StatTile from '../StatTile.svelte';

	let { service, data }: { service: ServiceView; data: ProwlarrData } = $props();
	const failing = $derived(data.indexers.filter((i) => i.status === 'failing').length);
	const sum = (k: 'queries_24h' | 'grabs_24h' | 'failures_24h') =>
		data.indexers.reduce((a, i) => a + i[k], 0);
</script>

{#each data.health as h (h.message)}
	<Banner tone="warning" text={h.message} />
{/each}

<div class="stats">
	<StatTile
		label="Indexers"
		value={data.indexers.filter((i) => i.enabled).length}
		trend="{failing} failing"
	/>
	<StatTile label="Queries · 24 h" value={sum('queries_24h')} />
	<StatTile label="Grabs · 24 h" value={sum('grabs_24h')} />
	<StatTile label="Failures · 24 h" value={sum('failures_24h')} />
</div>

<div class="row">
	<button
		class="btn btn-secondary btn-sm"
		disabled={!session.canAct(service.id)}
		onclick={() => void runAction(service.id, 'test_all', { label: 'Testing all indexers' })}
	>
		<Icon name="ph-plugs-connected" />Test all
	</button>
</div>

<div class="card tablecard">
	<table class="table">
		<thead>
			<tr
				><th>Indexer</th><th>Status</th><th>Privacy</th><th class="right">Response</th><th
					class="right">Queries</th
				><th class="right">Grabs</th><th class="right">Failures · 24 h</th></tr
			>
		</thead>
		<tbody>
			{#each data.indexers as i (i.id)}
				<tr>
					<td>{i.name}<span class="mono faint proto">{` ${i.protocol}`}</span></td>
					<td>
						<HealthDot
							health={i.status === 'ok' ? 'healthy' : i.status === 'failing' ? 'down' : 'disabled'}
							label={i.status === 'ok'
								? 'Working'
								: i.status === 'failing'
									? `Failing${i.disabled_till ? ` · retry ${time(i.disabled_till)}` : ''}`
									: 'Disabled'}
						/>
					</td>
					<td class="muted">{i.privacy}</td>
					<td class="right mono num">{ms(i.avg_response_ms)}</td>
					<td class="right mono num">{i.queries_24h}</td>
					<td class="right mono num">{i.grabs_24h}</td>
					<td
						class="right mono num"
						style:color={i.failures_24h > 0 ? 'var(--health-warning)' : undefined}
						>{i.failures_24h}</td
					>
				</tr>
			{/each}
		</tbody>
	</table>
</div>

<style>
	.stats {
		display: grid;
		gap: 10px;
		grid-template-columns: repeat(4, minmax(0, 1fr));
	}
	.tablecard {
		padding: 4px 8px;
		overflow-x: auto;
	}
	.proto {
		font-size: 11px;
	}
	@media (max-width: 900px) {
		.stats {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
	}
</style>
