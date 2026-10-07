<script lang="ts">
	// Seerr: pending requests (approve/decline), new request search, request history.
	import {
		api,
		errorMessage,
		type MediaRequest,
		type Schemas,
		type SeerrData,
		type ServiceView
	} from '#lib/api/client.js';
	import { runAction } from '#lib/actions.svelte.js';
	import { posterColors, relative } from '#lib/format.js';
	import { session } from '#lib/session.svelte.js';
	import EmptyState from '../EmptyState.svelte';
	import HealthBadge from '../HealthBadge.svelte';
	import Icon from '../Icon.svelte';
	import SearchField from '../SearchField.svelte';
	import StatTile from '../StatTile.svelte';

	let { service, data }: { service: ServiceView; data: SeerrData } = $props();
	const canAct = $derived(session.canAct(service.id));

	let query = $state('');
	let results = $state<Schemas['SearchResult'][] | null>(null);
	let searching = $state(false);
	let searchError = $state('');
	let timer: ReturnType<typeof setTimeout> | undefined;

	$effect(() => {
		const q = query.trim();
		clearTimeout(timer);
		if (q.length < 2) {
			results = null;
			return;
		}
		timer = setTimeout(async () => {
			searching = true;
			const r = await api.GET('/services/{id}/query/{name}', {
				params: { path: { id: service.id, name: 'search' }, query: { q } as never }
			});
			searching = false;
			if (r.error)
				searchError = errorMessage(
					new Error((r.error as { message?: string }).message ?? 'search failed')
				);
			else {
				searchError = '';
				results = r.data as unknown as Schemas['SearchResult'][];
			}
		}, 350);
	});

	const STATUS: Record<
		string,
		{
			health: 'healthy' | 'warning' | 'in_progress' | 'unknown' | 'down' | 'disabled';
			label: string;
		}
	> = {
		available: { health: 'healthy', label: 'Available' },
		partially_available: { health: 'healthy', label: 'Partly available' },
		processing: { health: 'in_progress', label: 'Processing' },
		pending: { health: 'warning', label: 'Pending' },
		deleted: { health: 'disabled', label: 'Deleted' },
		unknown: { health: 'unknown', label: 'Unknown' }
	};
	const label = (r: MediaRequest) =>
		r.status === 'declined'
			? { health: 'down' as const, label: 'Declined' }
			: r.status === 'pending'
				? STATUS.pending
				: (STATUS[r.media_status] ?? STATUS.unknown);
	const seasons = (r: MediaRequest) =>
		r.seasons.length ? ` · S${r.seasons.map((s) => String(s).padStart(2, '0')).join(', S')}` : '';
</script>

<div class="stats">
	<StatTile label="Pending" value={data.counts.pending} />
	<StatTile label="Processing" value={data.counts.processing} />
	<StatTile label="Available" value={data.counts.available} />
	<StatTile label="All requests" value={data.counts.total} />
</div>

<section>
	<h2 class="section-title">Waiting for approval</h2>
	{#if data.pending.length === 0}
		<EmptyState icon="ph-check-circle" title="No pending requests" />
	{:else}
		<ul class="list">
			{#each data.pending as r (r.id)}
				<li class="req">
					<div
						class="poster"
						style:background={r.poster_url
							? `center/cover url("${r.poster_url}"), linear-gradient(170deg, ${posterColors(r.title).join(',')})`
							: `linear-gradient(170deg, ${posterColors(r.title).join(',')})`}
					></div>
					<div class="txt">
						<span class="title"
							>{r.title}{#if r.year}<span class="muted">{` (${r.year})`}</span>{/if}</span
						>
						<span class="caption muted"
							>{r.media_type === 'tv' ? 'Series' : 'Movie'}{seasons(r)} · requested by {r.requested_by}
							· {relative(r.requested_at)}</span
						>
					</div>
					{#if canAct}
						<button
							class="btn btn-secondary btn-sm"
							onclick={() =>
								void runAction(service.id, 'decline', {
									label: `Declining ${r.title}`,
									args: { id: r.id }
								})}
						>
							<Icon name="ph-x" />Decline
						</button>
						<button
							class="btn btn-primary btn-sm"
							onclick={() =>
								void runAction(service.id, 'approve', {
									label: `Approving ${r.title}`,
									args: { id: r.id }
								})}
						>
							<Icon name="ph-check" />Approve
						</button>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
</section>

<section>
	<h2 class="section-title">New request</h2>
	<SearchField
		bind:value={query}
		placeholder="Search movies and series"
		label="Search for something to request"
		width="360px"
	/>
	{#if searching}<p class="caption muted pad">Searching…</p>{/if}
	{#if searchError}<p class="caption err pad">{searchError}</p>{/if}
	{#if results}
		{#if results.length === 0}
			<p class="caption muted pad">No results for "{query}".</p>
		{:else}
			<ul class="list">
				{#each results as r (r.media_type + r.tmdb_id)}
					{@const st = r.media_status ? STATUS[r.media_status] : null}
					<li class="req">
						<div
							class="poster"
							style:background={r.poster_url
								? `center/cover url("${r.poster_url}"), linear-gradient(170deg, ${posterColors(r.title).join(',')})`
								: `linear-gradient(170deg, ${posterColors(r.title).join(',')})`}
						></div>
						<div class="txt">
							<span class="title"
								>{r.title}{#if r.year}<span class="muted">{` (${r.year})`}</span>{/if}</span
							>
							<span class="caption muted ov"
								>{r.media_type === 'tv' ? 'Series' : 'Movie'}{r.overview
									? ` · ${r.overview}`
									: ''}</span
							>
						</div>
						{#if st}
							<HealthBadge health={st.health} label={st.label} small />
						{:else if canAct}
							<button
								class="btn btn-primary btn-sm"
								onclick={() =>
									void runAction(service.id, 'request', {
										label: `Requesting ${r.title}`,
										args: { media_type: r.media_type, tmdb_id: r.tmdb_id }
									}).then(() => (query = ''))}
							>
								<Icon name="ph-plus" />Request
							</button>
						{/if}
					</li>
				{/each}
			</ul>
		{/if}
	{/if}
</section>

<section>
	<h2 class="section-title">Recent requests</h2>
	<ul class="list">
		{#each data.recent as r (r.id)}
			{@const st = label(r)}
			<li class="req small">
				<div class="txt">
					<span class="title"
						>{r.title}{#if r.year}<span class="muted">{` (${r.year})`}</span>{/if}</span
					>
					<span class="caption muted"
						>{r.media_type === 'tv' ? 'Series' : 'Movie'}{seasons(r)} · {r.requested_by} · {relative(
							r.requested_at
						)}</span
					>
				</div>
				<HealthBadge health={st.health} label={st.label} small />
			</li>
		{/each}
	</ul>
</section>

<style>
	.stats {
		display: grid;
		gap: 10px;
		grid-template-columns: repeat(4, minmax(0, 1fr));
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
	.req {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 8px 12px;
		border-radius: var(--radius-md);
		background: var(--color-surface);
	}
	@media (max-width: 767px) {
		.req:not(.small) {
			flex-wrap: wrap;
		}
		.req:not(.small) .txt {
			flex: 1 1 calc(100% - 64px);
		}
		.req:not(.small) :global(.btn) {
			flex: 1;
			justify-content: center;
		}
	}
	.req.small {
		min-height: 48px;
	}
	.poster {
		width: 40px;
		aspect-ratio: 2 / 3;
		flex: none;
		border-radius: 5px;
		box-shadow: 0 0 0 1px var(--color-border);
	}
	.txt {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.title {
		font-size: 14px;
	}
	.ov {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.pad {
		padding: 4px 2px;
	}
	.err {
		color: var(--health-down);
	}
	@media (max-width: 900px) {
		.stats {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
	}
</style>
