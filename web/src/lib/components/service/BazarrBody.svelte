<script lang="ts">
	// Bazarr: missing subtitles (counts and list), search one or all, history.
	import type { BazarrData, ServiceView } from '#lib/api/client.js';
	import { runAction } from '#lib/actions.svelte.js';
	import { session } from '#lib/session.svelte.js';
	import EmptyState from '../EmptyState.svelte';
	import Icon from '../Icon.svelte';
	import StatTile from '../StatTile.svelte';

	let { service, data }: { service: ServiceView; data: BazarrData } = $props();
	const canAct = $derived(session.canAct(service.id));
</script>

<div class="stats">
	<StatTile label="Missing · episodes" value={data.missing_episodes} />
	<StatTile label="Missing · movies" value={data.missing_movies} />
	<StatTile label="Downloaded recently" value={data.history.length} />
</div>

<div class="row">
	<button
		class="btn btn-secondary btn-sm"
		disabled={!canAct}
		onclick={() =>
			void runAction(service.id, 'search_all', { label: 'Searching all missing subtitles' })}
	>
		<Icon name="ph-magnifying-glass" />Search all
	</button>
</div>

<section>
	<h2 class="section-title">Missing subtitles</h2>
	{#if data.wanted.length === 0}
		<EmptyState icon="ph-check-circle" title="No missing subtitles" />
	{:else}
		<ul class="list">
			{#each data.wanted as w (w.kind + w.title + (w.subtitle ?? ''))}
				<li class="item">
					<Icon
						name={w.kind === 'movie' ? 'ph-film-slate' : 'ph-television-simple'}
						color="var(--color-neutral-400)"
					/>
					<span class="title"
						>{w.title}{#if w.subtitle}<span class="muted">{` · ${w.subtitle}`}</span>{/if}</span
					>
					{#each w.languages as l (l)}<span class="tag tag-neutral">{l}</span>{/each}
					{#if canAct}
						<button
							class="btn btn-ghost btn-sm"
							onclick={() =>
								void runAction(service.id, 'search', {
									label: `Searching subtitles for ${w.title}`,
									args: { kind: w.kind, id: w.item_id }
								})}
						>
							<Icon name="ph-magnifying-glass" />Search
						</button>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
</section>

<section>
	<h2 class="section-title">History</h2>
	{#if data.history.length === 0}
		<p class="caption muted">Nothing downloaded recently.</p>
	{:else}
		<ul class="list">
			{#each data.history as h, i (i)}
				<li class="item">
					<Icon name="ph-subtitles" color="var(--color-neutral-400)" />
					<span class="title"
						>{h.title}<span class="caption muted">{` · ${h.description}`}</span></span
					>
					{#if h.when}<span class="caption faint">{h.when}</span>{/if}
				</li>
			{/each}
		</ul>
	{/if}
</section>

<style>
	.stats {
		display: grid;
		gap: 10px;
		grid-template-columns: repeat(3, minmax(0, 1fr));
	}
	section {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.item {
		display: flex;
		align-items: center;
		gap: 10px;
		min-height: 44px;
		padding: 6px 12px;
		border-radius: var(--radius-md);
		background: var(--color-surface);
		flex-wrap: wrap;
	}
	.title {
		flex: 1;
		min-width: 0;
		font-size: 13.5px;
	}
</style>
