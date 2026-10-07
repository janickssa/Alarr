<script lang="ts">
	// Sonarr and Radarr: queue, calendar, wanted/missing with search, library grid,
	// rescan/refresh/RSS sync, health banner.
	import { api, type ArrData, type LibraryItem, type ServiceView } from '#lib/api/client.js';
	import { imageUrl, runAction } from '#lib/actions.svelte.js';
	import { bytes, date, dateTime, relative, time } from '#lib/format.js';
	import { session } from '#lib/session.svelte.js';
	import Banner from '../Banner.svelte';
	import EmptyState from '../EmptyState.svelte';
	import Icon from '../Icon.svelte';
	import PosterCard from '../media/PosterCard.svelte';
	import SearchField from '../SearchField.svelte';
	import SegFilter from '../SegFilter.svelte';
	import StatTile from '../StatTile.svelte';

	let { service, data }: { service: ServiceView; data: ArrData } = $props();

	const sonarr = $derived(data.flavor === 'sonarr');
	const noun = $derived(sonarr ? 'series' : 'movies');
	const canAct = $derived(session.canAct(service.id));
	let tab = $state<'queue' | 'calendar' | 'missing' | 'library'>('queue');
	let library = $state<LibraryItem[] | null>(null);
	let libQuery = $state('');

	$effect(() => {
		if (tab === 'library' && library == null) {
			void api
				.GET('/services/{id}/query/{name}', {
					params: { path: { id: service.id, name: 'library' } }
				})
				.then((r) => (library = (r.data as unknown as LibraryItem[]) ?? []));
		}
	});

	const freeDisk = $derived(
		data.disks.reduce((m, d) => (d.free > m.free ? d : m), { free: 0, total: 0, path: '' })
	);
	const days = $derived.by(() => {
		const groups: { day: string; items: typeof data.calendar }[] = [];
		for (const c of data.calendar) {
			const day = date(c.at);
			const g = groups.find((x) => x.day === day);
			if (g) g.items.push(c);
			else groups.push({ day, items: [c] });
		}
		return groups;
	});
	const shownLibrary = $derived(
		(library ?? []).filter(
			(l) => !libQuery || l.title.toLowerCase().includes(libQuery.toLowerCase())
		)
	);

	function action(name: string, label: string, args?: Record<string, unknown>) {
		void runAction(service.id, name, { label, args });
	}
	const KIND: Record<string, string> = {
		cinema: 'In cinemas',
		digital: 'Digital release',
		physical: 'Physical release',
		airing: 'Airs'
	};
</script>

{#each data.health as h (h.message)}
	<Banner tone="warning" text="{service.name} reports: {h.message}">
		{#if h.wiki_url}<a
				class="btn btn-secondary"
				href={h.wiki_url}
				target="_blank"
				rel="noreferrer noopener">Details</a
			>{/if}
	</Banner>
{/each}

<div class="stats">
	<StatTile label="In queue" value={data.queue.length} />
	<StatTile label="Missing" value={data.missing_total} />
	<StatTile
		label="Library"
		value={data.library_count}
		unit={noun}
		trend={bytes(data.library_size)}
	/>
	<StatTile
		label="Free space"
		value={bytes(freeDisk.free).split(' ')[0]}
		unit={bytes(freeDisk.free).split(' ')[1]}
		trend={freeDisk.path ? `${freeDisk.path} · ${bytes(freeDisk.total)}` : undefined}
	/>
</div>

<div class="row actions">
	<button
		class="btn btn-secondary btn-sm"
		disabled={!canAct}
		onclick={() => action('rescan', `Rescanning ${service.name}`)}
		><Icon name="ph-folder-open" />Rescan</button
	>
	<button
		class="btn btn-secondary btn-sm"
		disabled={!canAct}
		onclick={() => action('refresh', `Refreshing ${service.name}`)}
		><Icon name="ph-arrow-clockwise" />Refresh</button
	>
	<button
		class="btn btn-secondary btn-sm"
		disabled={!canAct}
		onclick={() => action('rss_sync', 'RSS sync')}><Icon name="ph-rss" />RSS sync</button
	>
	<button
		class="btn btn-secondary btn-sm"
		disabled={!canAct}
		onclick={() => action('search_missing', `Searching all missing ${noun}`)}
		><Icon name="ph-magnifying-glass" />Search all missing</button
	>
</div>

<SegFilter
	label="Section"
	bind:value={tab}
	options={[
		{ value: 'queue', label: 'Queue', count: data.queue.length },
		{ value: 'calendar', label: 'Calendar', count: data.calendar.length },
		{ value: 'missing', label: 'Missing', count: data.missing_total },
		{ value: 'library', label: 'Library', count: data.library_count }
	]}
/>

{#if tab === 'queue'}
	{#if data.queue.length === 0}
		<EmptyState title="Queue is empty" detail="Nothing is downloading for {service.name}." />
	{:else}
		<ul class="list">
			{#each data.queue as q (q.id)}
				{@const pct = q.size ? 1 - q.size_left / q.size : 0}
				{@const warn = q.tracked_status === 'warning' || q.tracked_status === 'error'}
				<li class="item">
					<div class="top">
						<span class="title"
							>{q.title}{#if q.subtitle}<span class="muted">{` · ${q.subtitle}`}</span>{/if}</span
						>
						{#if warn}
							<span class="state warn"
								><Icon name="ph-warning" fill />{q.message ?? 'Needs attention'}</span
							>
						{:else}
							<span class="state prog"><Icon name="ph-circle-notch" />{q.status}</span>
						{/if}
					</div>
					<div
						class="track"
						role="progressbar"
						aria-label="Progress of {q.title}"
						aria-valuenow={Math.round(pct * 100)}
						aria-valuemin={0}
						aria-valuemax={100}
					>
						<div
							class="fill"
							style:width="{pct * 100}%"
							style:background={warn ? 'var(--health-warning)' : 'var(--health-progress)'}
						></div>
					</div>
					<div class="meta mono">
						<span>{Math.round(pct * 100)} %</span>
						<span>{bytes(q.size - q.size_left)} / {bytes(q.size)}</span>
						{#if q.eta}<span title={dateTime(q.eta)}>ETA {time(q.eta)}</span>{/if}
						<span class="rel" title={q.release}>{q.release}</span>
						{#if q.download_client}<span class="cl">{q.download_client}</span>{/if}
					</div>
				</li>
			{/each}
		</ul>
	{/if}
{:else if tab === 'calendar'}
	{#if days.length === 0}
		<EmptyState
			icon="ph-calendar-blank"
			title="Nothing coming up"
			detail="The next 14 days are empty."
		/>
	{:else}
		{#each days as d (d.day)}
			<section>
				<h3 class="overline day">{d.day}</h3>
				<ul class="list">
					{#each d.items as c (c.id + c.kind)}
						<li class="item cal">
							<span class="mono when">{sonarr ? time(c.at) : ''}</span>
							<span class="title"
								>{c.title}{#if c.subtitle}<span class="muted">{` · ${c.subtitle}`}</span>{/if}</span
							>
							<span class="tag tag-neutral">{KIND[c.kind] ?? c.kind}</span>
							{#if c.has_file}<span class="tag tag-accent">On disk</span>{/if}
						</li>
					{/each}
				</ul>
			</section>
		{/each}
	{/if}
{:else if tab === 'missing'}
	{#if data.missing.length === 0}
		<EmptyState icon="ph-check-circle" title="Nothing missing" />
	{:else}
		<ul class="list">
			{#each data.missing as m (m.id)}
				<li class="item cal">
					<span class="title"
						>{m.title}{#if m.subtitle}<span class="muted">{` · ${m.subtitle}`}</span>{/if}</span
					>
					{#if m.at}<span class="caption faint" title={dateTime(m.at)}>{relative(m.at)}</span>{/if}
					{#if canAct}
						<button
							class="btn btn-ghost btn-sm"
							onclick={() =>
								action('search_item', `Searching ${m.title}${m.subtitle ? ` ${m.subtitle}` : ''}`, {
									id: m.id
								})}
						>
							<Icon name="ph-magnifying-glass" />Search
						</button>
					{/if}
				</li>
			{/each}
		</ul>
		{#if data.missing_total > data.missing.length}
			<p class="caption muted">Showing the newest {data.missing.length} of {data.missing_total}.</p>
		{/if}
	{/if}
{:else}
	<SearchField
		bind:value={libQuery}
		placeholder="Filter {noun}"
		label="Filter library"
		width="260px"
	/>
	{#if library == null}
		<div class="posters">
			{#each Array(12), i (i)}<div class="skeleton poster-sk"></div>{/each}
		</div>
	{:else if shownLibrary.length === 0}
		<EmptyState title="No {noun} match" />
	{:else}
		<div class="posters">
			{#each shownLibrary as l (l.id)}
				<PosterCard
					title={l.title}
					meta="{l.year ?? ''}{sonarr
						? ` · ${l.have}/${l.total} ep`
						: l.complete
							? ''
							: ' · missing'}"
					image={imageUrl(service.id, l.poster) ?? undefined}
				/>
			{/each}
		</div>
	{/if}
{/if}

<style>
	.stats {
		display: grid;
		gap: 10px;
		grid-template-columns: repeat(4, minmax(0, 1fr));
	}
	.actions {
		gap: 8px;
		flex-wrap: wrap;
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
		background: var(--color-surface);
		border-radius: var(--radius-md);
		padding: 10px 12px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.item.cal {
		flex-direction: row;
		align-items: center;
		gap: 10px;
		min-height: 44px;
		padding: 6px 12px;
	}
	.top {
		display: flex;
		justify-content: space-between;
		gap: 10px;
	}
	.title {
		flex: 1;
		min-width: 0;
		font-size: 13.5px;
	}
	.state {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: 11.5px;
		flex: none;
	}
	.warn {
		color: var(--health-warning);
	}
	.prog {
		color: var(--health-progress);
	}
	.track {
		height: 4px;
		border-radius: 2px;
		background: var(--color-neutral-800);
	}
	.fill {
		height: 100%;
		border-radius: 2px;
	}
	.meta {
		display: flex;
		gap: 14px;
		font-size: 11.5px;
		color: var(--color-neutral-400);
		min-width: 0;
	}
	.rel {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		min-width: 0;
	}
	.cl {
		margin-left: auto;
		flex: none;
	}
	.day {
		margin: 6px 0;
	}
	.when {
		width: 44px;
		font-size: 12px;
		color: var(--color-neutral-400);
	}
	.posters {
		display: grid;
		gap: 14px;
		grid-template-columns: repeat(auto-fill, minmax(110px, 1fr));
	}
	.posters :global(.poster-card) {
		width: auto;
	}
	.poster-sk {
		aspect-ratio: 2 / 3;
		border-radius: var(--radius-md);
	}
	@media (max-width: 900px) {
		.stats {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
	}
</style>
