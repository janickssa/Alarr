<script lang="ts">
	// Authentik (read-only): logins and failed logins in 24 h, recent events, active
	// sessions and outpost health.
	import type { AuthentikData } from '#lib/api/client.js';
	import Banner from '../Banner.svelte';
	import EmptyState from '../EmptyState.svelte';
	import HealthDot from '../HealthDot.svelte';
	import Icon from '../Icon.svelte';
	import RelativeTime from '../RelativeTime.svelte';
	import StatTile from '../StatTile.svelte';

	let { data }: { data: AuthentikData } = $props();

	const ACTION: Record<string, { label: string; icon: string; color?: string }> = {
		login: { label: 'Signed in', icon: 'ph-sign-in', color: 'var(--health-healthy)' },
		login_failed: { label: 'Failed sign-in', icon: 'ph-warning', color: 'var(--health-warning)' },
		logout: { label: 'Signed out', icon: 'ph-sign-out' },
		authorize_application: { label: 'Opened', icon: 'ph-app-window' },
		suspicious_request: {
			label: 'Suspicious request',
			icon: 'ph-shield-warning',
			color: 'var(--health-down)'
		},
		password_set: { label: 'Changed password', icon: 'ph-password' },
		user_write: { label: 'Updated user', icon: 'ph-user-gear' },
		model_created: { label: 'Created', icon: 'ph-plus-circle' },
		model_updated: { label: 'Updated', icon: 'ph-pencil-simple' },
		model_deleted: { label: 'Deleted', icon: 'ph-trash' }
	};
	const label = (a: string) => ACTION[a]?.label ?? a.replaceAll('_', ' ');
</script>

{#if data.update_available}
	<Banner
		tone="progress"
		icon="ph-arrow-circle-up"
		text="Authentik {data.update_available} is available"
	/>
{/if}

<div class="stats">
	<StatTile label="Sign-ins · 24 h" value="{data.logins_24h}{data.logins_capped ? '+' : ''}" />
	<StatTile
		label="Failed sign-ins · 24 h"
		value={data.failed_24h}
		trend={data.failed_24h > 0 ? 'check the events below' : undefined}
	/>
	<StatTile label="Active sessions" value={data.sessions ?? '–'} />
	<StatTile
		label="Outposts"
		value={data.outposts.length}
		trend="{data.outposts.filter((o) => !o.healthy).length} not checking in"
	/>
</div>

<div class="two">
	<section>
		<h2 class="section-title">Recent events</h2>
		{#if data.events.length === 0}
			<EmptyState icon="ph-list-bullets" title="No events" />
		{:else}
			<ul class="card list">
				{#each data.events as e, i (i)}
					<li>
						<Icon name={ACTION[e.action]?.icon ?? 'ph-dot'} color={ACTION[e.action]?.color} />
						<span class="txt"
							><span
								>{label(e.action)}{#if e.app}<span class="muted">{` ${e.app}`}</span>{/if}</span
							><span class="caption faint">{[e.user, e.client_ip].filter(Boolean).join(' · ')}</span
							></span
						>
						<RelativeTime value={e.at} />
					</li>
				{/each}
			</ul>
		{/if}
	</section>
	<section>
		<h2 class="section-title">Outposts</h2>
		{#if data.outposts.length === 0}
			<EmptyState icon="ph-broadcast" title="No outposts" />
		{:else}
			<ul class="card list">
				{#each data.outposts as o (o.id)}
					<li>
						<span class="txt"
							><span>{o.name}</span><span class="caption faint"
								>{o.kind}{o.version ? ` · ${o.version}` : ''}{o.outdated ? ' · outdated' : ''}</span
							></span
						>
						<span class="seen">
							<HealthDot
								health={o.healthy ? 'healthy' : 'down'}
								label={o.healthy ? 'Checking in' : 'Not checking in'}
							/>
							{#if o.last_seen}<span class="caption faint"
									><RelativeTime value={o.last_seen} /></span
								>{:else}<span class="caption faint">never seen</span>{/if}
						</span>
					</li>
				{/each}
			</ul>
		{/if}
	</section>
</div>

<style>
	.stats {
		display: grid;
		gap: 10px;
		grid-template-columns: repeat(4, minmax(0, 1fr));
	}
	.two {
		display: grid;
		gap: 16px;
		grid-template-columns: minmax(0, 3fr) minmax(0, 2fr);
		align-items: start;
	}
	.section-title {
		margin-bottom: 8px;
	}
	.list {
		list-style: none;
		margin: 0;
		padding: 4px 0;
	}
	li {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 14px;
		font-size: 13px;
	}
	li + li {
		border-top: 1px solid var(--color-divider);
	}
	.txt {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}
	.seen {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		gap: 2px;
	}
	@media (max-width: 900px) {
		.stats {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
		.two {
			grid-template-columns: 1fr;
		}
	}
</style>
