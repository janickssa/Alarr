<script lang="ts">
	// Media: now playing across Jellyfin instances and Navidrome, request tracker
	// (Requested → Approved → Grabbed → Downloading → Imported → Available), combined
	// calendar and recently added.
	import { resolve } from '$app/paths';
	import { onMount } from 'svelte';
	import { api, errorMessage, unwrap, type MediaOverview } from '#lib/api/client.js';
	import { imageUrl } from '#lib/actions.svelte.js';
	import EmptyState from '#lib/components/EmptyState.svelte';
	import ErrorState from '#lib/components/ErrorState.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import NowPlayingRow from '#lib/components/media/NowPlayingRow.svelte';
	import PosterCard from '#lib/components/media/PosterCard.svelte';
	import RequestTracker from '#lib/components/media/RequestTracker.svelte';
	import SkeletonCard from '#lib/components/SkeletonCard.svelte';
	import StatTile from '#lib/components/StatTile.svelte';
	import { clock, date, posterColors, relative, time } from '#lib/format.js';
	import { refetchOnLive } from '#lib/live.svelte.js';
	import { session } from '#lib/session.svelte.js';

	let data = $state<MediaOverview | null>(null);
	let error = $state('');

	async function load() {
		try {
			data = unwrap(await api.GET('/media/overview'));
			error = '';
		} catch (e) {
			error = errorMessage(e);
		}
	}
	onMount(load);
	refetchOnLive(
		() => session.services.filter((s) => s.category === 'media').map((s) => s.id),
		load,
		{ minInterval: 3000, actions: true }
	);

	const days = $derived.by(() => {
		const groups: { day: string; items: MediaOverview['calendar'] }[] = [];
		for (const c of data?.calendar ?? []) {
			const day = date(c.item.at);
			const g = groups.find((x) => x.day === day);
			if (g) g.items.push(c);
			else groups.push({ day, items: [c] });
		}
		return groups;
	});
	const hasMedia = $derived(session.services.some((s) => s.category === 'media' && s.enabled));
	const KIND: Record<string, string> = {
		cinema: 'In cinemas',
		digital: 'Digital',
		physical: 'Physical',
		airing: 'Airs'
	};
</script>

<svelte:head><title>Media</title></svelte:head>

<div class="page">
	<div class="page-header"><h1 class="page-title">Media</h1></div>

	{#if !hasMedia}
		<EmptyState
			icon="ph-film-strip"
			title="No media services"
			detail="Add Jellyfin, Sonarr, Radarr, Seerr or qBittorrent in Settings."
		/>
	{:else if error && !data}
		<ErrorState title="Can't load media" detail={error} onretry={load} />
	{:else if !data}
		<div class="grid">
			{#each Array(4), i (i)}<SkeletonCard />{/each}
		</div>
	{:else}
		<div class="stats">
			<StatTile
				label="Playing now"
				value={data.counts.streams}
				trend="{data.counts.transcodes} transcode{data.counts.transcodes === 1 ? '' : 's'}"
			/>
			<StatTile label="Downloading" value={data.counts.downloading} />
			<StatTile label="Waiting for approval" value={data.counts.pending_requests} />
			<StatTile label="Coming up · 14 d" value={data.calendar.length} />
		</div>

		<section>
			<h2 class="section-title">Now playing</h2>
			{#if data.now_playing.length === 0}
				<EmptyState icon="ph-television-simple" title="Nothing playing" />
			{:else}
				<div class="col">
					{#each data.now_playing as n, i (i)}
						{#if n.kind === 'video'}
							<NowPlayingRow
								title="{n.title}{n.subtitle ? ` · ${n.subtitle}` : ''}"
								detail="{n.user} · {n.device ?? ''} · {n.detail ?? ''} · {n.service_name}"
								progress={n.progress ?? 0}
								position="{clock(n.position_s)} / {clock(n.runtime_s)}"
								transcode={n.play_method === 'Transcode'}
								reason={n.transcode_reason ?? undefined}
								tags={n.remote ? ['Remote'] : []}
							/>
						{:else}
							<div class="music">
								<div class="cover">
									<Icon name="ph-music-notes" size={20} color="oklch(0.84 0.07 10)" />
								</div>
								<div class="mtxt">
									<span>{n.title}</span>
									<span class="caption muted"
										>{n.subtitle ?? ''} · {n.user}{n.device ? ` · ${n.device}` : ''} · {n.service_name}</span
									>
								</div>
							</div>
						{/if}
					{/each}
				</div>
			{/if}
		</section>

		<section>
			<div class="row">
				<h2 class="section-title">Requests</h2>
				<span class="spacer"></span>
				{#if data.requests[0]}
					<a
						class="btn btn-ghost btn-sm"
						href={resolve('/services/[id]', { id: data.requests[0].seerr_id })}
						>Seerr<Icon name="ph-caret-right" /></a
					>
				{/if}
			</div>
			{#if data.requests.length === 0}
				<EmptyState
					icon="ph-ticket"
					title="No requests"
					detail="Requests from Seerr show up here with their progress."
				/>
			{:else}
				<div class="col">
					{#each data.requests as t (t.seerr_id + t.request.id)}
						<RequestTracker
							title="{t.request.title}{t.request.year ? ` (${t.request.year})` : ''}{t.request
								.media_type === 'tv' && t.request.seasons.length
								? ` · S${t.request.seasons.map((s) => String(s).padStart(2, '0')).join(', S')}`
								: ''}"
							meta="requested by {t.request.requested_by} · {relative(t.request.requested_at)}"
							steps={t.steps}
						/>
					{/each}
				</div>
			{/if}
		</section>

		<div class="two">
			<section>
				<h2 class="section-title">Calendar</h2>
				{#if days.length === 0}
					<EmptyState icon="ph-calendar-blank" title="Nothing coming up" />
				{:else}
					{#each days as d (d.day)}
						<h3 class="overline day">{d.day}</h3>
						<ul class="list">
							{#each d.items as c (c.service_id + c.item.id + c.item.kind)}
								<li class="item">
									<Icon
										name={c.flavor === 'sonarr' ? 'ph-television-simple' : 'ph-film-slate'}
										color="var(--color-neutral-400)"
									/>
									<span class="mono when">{c.flavor === 'sonarr' ? time(c.item.at) : ''}</span>
									<span class="spacer"
										>{c.item.title}<span class="muted"
											>{c.item.subtitle ? ` · ${c.item.subtitle}` : ''}</span
										></span
									>
									<span class="tag tag-neutral">{KIND[c.item.kind] ?? c.item.kind}</span>
								</li>
							{/each}
						</ul>
					{/each}
				{/if}
			</section>
			<section>
				<h2 class="section-title">Recently added</h2>
				{#if data.recently_added.length === 0}
					<EmptyState title="Nothing new" />
				{:else}
					<div class="posters">
						{#each data.recently_added as r (r.service_id + r.item.id)}
							<PosterCard
								title={r.item.series_name ?? r.item.name}
								meta={r.item.series_name
									? r.item.name
									: `${r.item.item_type} · ${r.item.year ?? ''}`}
								image={imageUrl(r.service_id, `/Items/${r.item.id}/Images/Primary`) ?? undefined}
								colors={posterColors(r.item.series_name ?? r.item.name)}
								isNew={r.item.added_at
									? Date.now() - new Date(r.item.added_at).getTime() < 86_400_000
									: false}
							/>
						{/each}
					</div>
				{/if}
			</section>
		</div>
	{/if}
</div>

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
	.col {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.music {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 10px 12px;
		border-radius: var(--radius-md);
		background: var(--color-surface);
	}
	.cover {
		width: 40px;
		height: 40px;
		border-radius: 6px;
		background: oklch(0.33 0.045 10);
		display: grid;
		place-items: center;
	}
	.mtxt {
		display: flex;
		flex-direction: column;
		font-size: 14px;
	}
	.two {
		display: grid;
		gap: var(--gap);
		grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
		align-items: start;
	}
	.day {
		margin: 6px 0 2px;
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
		min-height: 40px;
		padding: 4px 12px;
		border-radius: var(--radius-md);
		background: var(--color-surface);
		font-size: 13.5px;
	}
	.when {
		width: 40px;
		font-size: 12px;
		color: var(--color-neutral-400);
	}
	.posters {
		display: grid;
		gap: 14px;
		grid-template-columns: repeat(auto-fill, minmax(100px, 1fr));
	}
	.posters :global(.poster-card) {
		width: auto;
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
