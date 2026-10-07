<script lang="ts">
	// Jellyfin: streams with transcode reason and bitrate, libraries with scans, recently
	// added and users.
	import type { JellyfinData, ServiceView } from '#lib/api/client.js';
	import { imageUrl, runAction } from '#lib/actions.svelte.js';
	import { bitrate, clock, posterColors, relative, count } from '#lib/format.js';
	import { session } from '#lib/session.svelte.js';
	import EmptyState from '../EmptyState.svelte';
	import Icon from '../Icon.svelte';
	import NowPlayingRow from '../media/NowPlayingRow.svelte';
	import PosterCard from '../media/PosterCard.svelte';
	import StatTile from '../StatTile.svelte';

	let { service, data }: { service: ServiceView; data: JellyfinData } = $props();
	const canAct = $derived(session.canAct(service.id));
	const total = $derived(data.streams.reduce((a, s) => a + (s.bitrate ?? 0), 0));
	const transcodes = $derived(data.streams.filter((s) => s.play_method === 'Transcode').length);

	function reason(r: string) {
		return r.replace(/([a-z])([A-Z])/g, '$1 $2').toLowerCase();
	}
	function detail(s: JellyfinData['streams'][number]) {
		const parts = [s.user, s.device || s.client];
		if (s.play_method === 'Transcode' && s.transcode) {
			parts.push(
				`${s.source ?? ''} → ${(s.transcode.video_codec ?? '').toUpperCase().replace('H264', 'H.264')}`.trim()
			);
		} else if (s.source) {
			parts.push(
				`${s.source} · ${s.play_method === 'DirectPlay' ? 'direct play' : 'direct stream'}`
			);
		}
		if (s.bitrate) parts.push(bitrate(s.bitrate));
		return parts.filter(Boolean).join(' · ');
	}
	const TYPE: Record<string, string> = {
		movies: 'Movies',
		tvshows: 'Shows',
		music: 'Music',
		books: 'Books',
		homevideos: 'Home videos'
	};
</script>

<div class="stats">
	<StatTile
		label="Active streams"
		value={data.streams.length}
		trend="{transcodes} transcode{transcodes === 1 ? '' : 's'}"
	/>
	<StatTile label="Bandwidth" value={total ? bitrate(total).split(' ')[0] : '0'} unit="Mbit/s" />
	<StatTile
		label="Movies"
		value={data.counts.movies}
		trend="{data.counts.series} series · {data.counts.episodes} episodes"
	/>
	<StatTile
		label="Music"
		value={data.counts.albums}
		unit="albums"
		trend="{count(data.counts.songs)} songs"
	/>
</div>

<div class="row actions">
	<button
		class="btn btn-secondary btn-sm"
		disabled={!canAct}
		onclick={() =>
			void runAction(service.id, 'scan_all', {
				label: `Scanning all libraries on ${service.name}`
			})}
	>
		<Icon name="ph-arrows-clockwise" />Scan all libraries
	</button>
</div>

<section>
	<h2 class="section-title">Now playing</h2>
	{#if data.streams.length === 0}
		<EmptyState icon="ph-television-simple" title="Nobody is watching" />
	{:else}
		<div class="col">
			{#each data.streams as s (s.session_id)}
				<NowPlayingRow
					title="{s.title}{s.subtitle ? ` · ${s.subtitle}` : ''}"
					detail={detail(s)}
					progress={s.position_s && s.runtime_s ? s.position_s / s.runtime_s : 0}
					position="{clock(s.position_s)} / {clock(s.runtime_s)}"
					transcode={s.play_method === 'Transcode'}
					reason={s.transcode?.reasons.length
						? s.transcode.reasons.map(reason).join(', ')
						: undefined}
					tags={[...(s.paused ? ['Paused'] : []), ...(s.remote ? ['Remote'] : [])]}
				/>
			{/each}
		</div>
	{/if}
</section>

<div class="two">
	<section class="card">
		<h2 class="section-title">Libraries</h2>
		<table class="table">
			<tbody>
				{#each data.libraries as l (l.id)}
					<tr>
						<td>{l.name}</td>
						<td class="muted"
							>{(TYPE[l.collection_type ?? ''] ?? l.collection_type ?? '') === l.name
								? ''
								: (TYPE[l.collection_type ?? ''] ?? l.collection_type ?? '')}</td
						>
						<td class="right mono num">{l.item_count ?? '–'}</td>
						<td class="right">
							{#if canAct}
								<button
									class="btn btn-ghost btn-sm"
									onclick={() =>
										void runAction(service.id, 'scan_library', {
											label: `Scanning ${l.name}`,
											args: { id: l.id }
										})}
								>
									<Icon name="ph-arrows-clockwise" />Scan
								</button>
							{/if}
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
	</section>
	<section class="card">
		<h2 class="section-title">Users</h2>
		<ul class="users">
			{#each data.users as u (u.name)}
				<li>
					<Icon name="ph-user-circle" size={18} color="var(--color-neutral-400)" />
					<span class="spacer"
						>{u.name}{#if u.is_admin}<span class="tag tag-accent adm">admin</span>{/if}</span
					>
					<span class="caption faint">{u.last_activity ? relative(u.last_activity) : 'never'}</span>
				</li>
			{/each}
		</ul>
	</section>
</div>

<section>
	<h2 class="section-title">Recently added</h2>
	<div class="posters">
		{#each data.recent as r (r.id)}
			<PosterCard
				title={r.series_name ?? r.name}
				meta={r.series_name
					? r.name
					: `${r.item_type === 'Movie' ? 'Movie' : r.item_type} · ${r.year ?? ''}`}
				image={imageUrl(service.id, `/Items/${r.id}/Images/Primary`) ?? undefined}
				colors={posterColors(r.series_name ?? r.name)}
				isNew={r.added_at ? Date.now() - new Date(r.added_at).getTime() < 86_400_000 : false}
			/>
		{/each}
	</div>
</section>

<style>
	.stats {
		display: grid;
		gap: 10px;
		grid-template-columns: repeat(4, minmax(0, 1fr));
	}
	.actions {
		gap: 8px;
	}
	section > .section-title {
		margin-bottom: 8px;
	}
	.col {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.two {
		display: grid;
		gap: var(--gap);
		grid-template-columns: minmax(0, 3fr) minmax(0, 2fr);
		align-items: start;
	}
	.users {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.users li {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 13px;
		min-height: 30px;
	}
	.adm {
		margin-left: 6px;
		font-size: 10px;
	}
	.posters {
		display: grid;
		gap: 14px;
		grid-template-columns: repeat(auto-fill, minmax(110px, 1fr));
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
