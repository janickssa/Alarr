<script lang="ts">
	// Nginx Proxy Manager (read-only): proxy hosts and certificates with days left
	// (< 14 d amber, < 3 d or expired red).
	import type { NpmData } from '#lib/api/client.js';
	import { date } from '#lib/format.js';
	import EmptyState from '../EmptyState.svelte';
	import HealthDot from '../HealthDot.svelte';
	import Icon from '../Icon.svelte';
	import StatTile from '../StatTile.svelte';

	let { data }: { data: NpmData } = $props();

	const WARN = 14;
	const CRITICAL = 3;
	function tone(days: number | null | undefined): string | undefined {
		if (days == null) return undefined;
		if (days < CRITICAL) return 'var(--health-down-text)';
		if (days < WARN) return 'var(--health-warning)';
		return undefined;
	}
	function daysLabel(days: number | null | undefined): string {
		if (days == null) return '–';
		if (days < 0) return 'expired';
		if (days === 0) return 'today';
		return `${days} d`;
	}
	const certName = $derived(Object.fromEntries(data.certificates.map((c) => [c.id, c])));
	const enabled = $derived(data.hosts.filter((h) => h.enabled).length);
	const expiring = $derived(
		data.certificates.filter((c) => c.hosts > 0 && c.days_left != null && c.days_left < WARN)
	);
</script>

<div class="stats">
	<StatTile label="Proxy hosts" value={enabled} trend="{data.hosts.length - enabled} disabled" />
	<StatTile label="With HTTPS" value={data.hosts.filter((h) => h.certificate_id != null).length} />
	<StatTile label="Certificates" value={data.certificates.length} />
	<StatTile
		label="Expiring < 14 d"
		value={expiring.length}
		trend={expiring.length ? 'in use' : 'none in use'}
	/>
</div>

<section>
	<h2 class="section-title">Certificates</h2>
	{#if data.certificates.length === 0}
		<EmptyState icon="ph-certificate" title="No certificates" />
	{:else}
		<div class="card tablecard">
			<table class="table">
				<thead>
					<tr
						><th>Certificate</th><th class="hide-sm">Provider</th><th class="hide-sm">Expires</th
						><th class="right">Days left</th><th class="right">Hosts</th></tr
					>
				</thead>
				<tbody>
					{#each data.certificates as c (c.id)}
						<tr>
							<td
								><span>{c.name}</span>{#if c.domains.length > 1 || c.domains[0] !== c.name}<span
										class="caption faint block">{c.domains.join(', ')}</span
									>{/if}</td
							>
							<td class="muted hide-sm"
								>{c.provider === 'letsencrypt' ? "Let's Encrypt" : 'Custom'}</td
							>
							<td class="mono hide-sm">{date(c.expires_at)}</td>
							<td class="right mono num" style:color={tone(c.days_left)}>
								{#if c.days_left != null && c.days_left < WARN}<Icon
										name="ph-warning"
										color={tone(c.days_left)}
									/>{/if}
								{daysLabel(c.days_left)}
							</td>
							<td class="right mono num">{c.hosts}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</section>

<section>
	<h2 class="section-title">Proxy hosts</h2>
	{#if data.hosts.length === 0}
		<EmptyState icon="ph-arrows-split" title="No proxy hosts" />
	{:else}
		<div class="card tablecard">
			<table class="table">
				<thead>
					<tr
						><th>Domain</th><th class="hide-sm">Forwards to</th><th>Status</th><th>Certificate</th
						></tr
					>
				</thead>
				<tbody>
					{#each data.hosts as h (h.id)}
						{@const cert = h.certificate_id != null ? certName[h.certificate_id] : null}
						<tr>
							<td
								>{h.domains[0] ?? '–'}{#if h.domains.length > 1}<span class="caption faint"
										>{` +${h.domains.length - 1}`}</span
									>{/if}</td
							>
							<td class="mono muted hide-sm">{h.forward}</td>
							<td>
								<HealthDot
									health={!h.enabled ? 'disabled' : h.online === false ? 'down' : 'healthy'}
									label={!h.enabled ? 'Disabled' : h.online === false ? 'Offline' : 'Online'}
								/>
							</td>
							<td>
								{#if cert}
									<span style:color={tone(cert.days_left)}
										><Icon name="ph-lock-simple" />
										{daysLabel(cert.days_left)}{h.ssl_forced ? ' · forced' : ''}</span
									>
								{:else}
									<span class="faint">HTTP only</span>
								{/if}
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</section>

<style>
	.stats {
		display: grid;
		gap: 10px;
		grid-template-columns: repeat(4, minmax(0, 1fr));
	}
	.section-title {
		margin-bottom: 8px;
	}
	.tablecard {
		padding: 4px 8px;
		overflow-x: auto;
	}
	.block {
		display: block;
	}
	@media (max-width: 900px) {
		.stats {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
	}
</style>
