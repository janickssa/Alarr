<script lang="ts">
	// Technitium DNS: 24 h stats, allowed vs blocked chart, top lists, pause blocking with a
	// countdown and resume.
	import type { ServiceView, TechnitiumData } from '#lib/api/client.js';
	import { runAction } from '#lib/actions.svelte.js';
	import { count, pct } from '#lib/format.js';
	import { session } from '#lib/session.svelte.js';
	import Banner from '../Banner.svelte';
	import Icon from '../Icon.svelte';
	import StatTile from '../StatTile.svelte';
	import TimeChart from '../TimeChart.svelte';

	let { service, data }: { service: ServiceView; data: TechnitiumData } = $props();
	const canAct = $derived(session.canAct(service.id));

	// Ticks once a second while blocking is paused, for the countdown.
	let now = $state(Date.now());
	$effect(() => {
		if (!data.paused_until) return;
		const t = setInterval(() => (now = Date.now()), 1000);
		return () => clearInterval(t);
	});
	const left = $derived(
		data.paused_until ? Math.max(0, new Date(data.paused_until).getTime() - now) : 0
	);
	const countdown = $derived.by(() => {
		const s = Math.ceil(left / 1000);
		return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
	});

	const PAUSES = [
		{ minutes: 5, label: '5 min' },
		{ minutes: 30, label: '30 min' },
		{ minutes: 60, label: '1 h' }
	];
	const times = $derived(data.chart.map((p) => new Date(p.at).getTime() / 1000));
	const blockedFrac = $derived(data.total_queries ? data.blocked / data.total_queries : null);
	const lists = $derived([
		{ title: 'Top blocked', icon: 'ph-prohibit', items: data.top_blocked },
		{ title: 'Top domains', icon: 'ph-globe-simple', items: data.top_domains },
		{ title: 'Top clients', icon: 'ph-devices', items: data.top_clients }
	]);
</script>

{#if data.paused_until}
	<Banner tone="warning" icon="ph-pause-circle" text="Blocking is paused · resumes in {countdown}">
		<button
			class="btn btn-secondary btn-sm"
			disabled={!canAct}
			onclick={() => void runAction(service.id, 'resume_blocking', { label: 'Resuming blocking' })}
			><Icon name="ph-play" />Resume now</button
		>
	</Banner>
{:else if !data.blocking_enabled}
	<Banner tone="warning" text="Blocking is turned off in Technitium">
		<button
			class="btn btn-secondary btn-sm"
			disabled={!canAct}
			onclick={() =>
				void runAction(service.id, 'resume_blocking', { label: 'Turning blocking on' })}
			><Icon name="ph-play" />Turn on</button
		>
	</Banner>
{/if}

<div class="stats">
	<StatTile label="Queries · 24 h" value={count(data.total_queries)} />
	<StatTile
		label="Blocked · 24 h"
		value={count(data.blocked)}
		trend={blockedFrac != null ? pct(blockedFrac) : undefined}
	/>
	<StatTile
		label="Cached"
		value={data.total_queries ? pct(data.cached / data.total_queries) : '–'}
	/>
	<StatTile
		label="Clients"
		value={data.clients}
		trend="{count(data.block_list_zones)} blocked domains"
	/>
</div>

{#if data.blocking_enabled && !data.paused_until}
	<div class="row" role="group" aria-label="Pause blocking">
		<span class="caption muted"><Icon name="ph-pause" /> Pause blocking</span>
		{#each PAUSES as p (p.minutes)}
			<button
				class="btn btn-secondary btn-sm"
				disabled={!canAct}
				onclick={() =>
					void runAction(service.id, 'pause_blocking', {
						label: `Pausing blocking for ${p.label}`,
						args: { minutes: p.minutes }
					})}>{p.label}</button
			>
		{/each}
	</div>
{/if}

<TimeChart
	title="Queries · last 24 h"
	ranges={false}
	{times}
	height={160}
	series={[
		{ label: 'Allowed', values: data.chart.map((p) => p.total - p.blocked), format: count },
		{ label: 'Blocked', values: data.chart.map((p) => p.blocked), format: count }
	]}
/>

<div class="lists">
	{#each lists as l (l.title)}
		<section class="card list">
			<h2 class="section-title"><Icon name={l.icon} /> {l.title}</h2>
			{#if l.items.length === 0}
				<p class="caption faint">Nothing yet</p>
			{:else}
				<ol>
					{#each l.items as e (e.name)}
						{@const max = l.items[0].hits || 1}
						<li>
							<span class="name"
								><span class="mono">{e.name}</span>{#if e.domain}<span class="caption faint"
										>{e.domain}</span
									>{/if}</span
							>
							<span class="mono num hits">{count(e.hits)}</span>
							<span class="bar" style:width="{(e.hits / max) * 100}%"></span>
						</li>
					{/each}
				</ol>
			{/if}
		</section>
	{/each}
</div>

<style>
	.stats {
		display: grid;
		gap: 10px;
		grid-template-columns: repeat(4, minmax(0, 1fr));
	}
	.row {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
	}
	.lists {
		display: grid;
		gap: 12px;
		grid-template-columns: repeat(3, minmax(0, 1fr));
	}
	.list {
		padding: 14px;
	}
	.section-title {
		display: flex;
		align-items: center;
		gap: 6px;
		margin-bottom: 8px;
	}
	ol {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	li {
		position: relative;
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 5px 8px;
		font-size: 12px;
		border-radius: 6px;
		overflow: hidden;
	}
	.bar {
		position: absolute;
		inset: 0 auto 0 0;
		background: var(--color-accent);
		opacity: 0.1;
		pointer-events: none;
	}
	.name {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		overflow-wrap: anywhere;
	}
	.hits {
		color: var(--color-neutral-300);
	}
	@media (max-width: 1100px) {
		.lists {
			grid-template-columns: 1fr;
		}
	}
	@media (max-width: 900px) {
		.stats {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
	}
</style>
