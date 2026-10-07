<script lang="ts">
	import type { QbittorrentData, ServiceView, Torrent } from '#lib/api/client.js';
	import { runAction } from '#lib/actions.svelte.js';
	import { bytes, duration, rate } from '#lib/format.js';
	import { session } from '#lib/session.svelte.js';
	import Dialog from '../Dialog.svelte';
	import EmptyState from '../EmptyState.svelte';
	import Icon from '../Icon.svelte';
	import SearchField from '../SearchField.svelte';
	import SegFilter from '../SegFilter.svelte';
	import StatTile from '../StatTile.svelte';
	import Switch from '../Switch.svelte';
	import TimeChart from '../TimeChart.svelte';

	let { service, data }: { service: ServiceView; data: QbittorrentData } = $props();

	type Filter = 'all' | 'downloading' | 'seeding' | 'stopped' | 'errored';
	let filter = $state<Filter>('all');
	let query = $state('');
	let deleting = $state<Torrent | null>(null);
	let deleteFiles = $state(false);
	const canAct = $derived(session.canAct(service.id));

	const FILTER_STATES: Record<Exclude<Filter, 'all'>, string[]> = {
		downloading: ['downloading', 'stalled', 'metadata'],
		seeding: ['seeding'],
		stopped: ['stopped'],
		errored: ['errored']
	};
	const shown = $derived(
		data.torrents.filter(
			(t) =>
				(filter === 'all' || FILTER_STATES[filter].includes(t.state)) &&
				(!query || t.name.toLowerCase().includes(query.toLowerCase()))
		)
	);
	const times = $derived(data.history.map((p) => new Date(p.at).getTime() / 1000));

	const STATE: Record<string, { label: string; color: string; icon: string }> = {
		downloading: { label: 'Downloading', color: 'var(--health-progress)', icon: 'ph-arrow-down' },
		stalled: { label: 'Stalled', color: 'var(--health-warning)', icon: 'ph-warning' },
		metadata: {
			label: 'Fetching metadata',
			color: 'var(--health-progress)',
			icon: 'ph-circle-notch'
		},
		seeding: { label: 'Seeding', color: 'var(--health-healthy)', icon: 'ph-arrow-up' },
		stopped: { label: 'Stopped', color: 'var(--color-neutral-400)', icon: 'ph-pause' },
		queued: { label: 'Queued', color: 'var(--color-neutral-400)', icon: 'ph-clock' },
		checking: { label: 'Checking', color: 'var(--health-progress)', icon: 'ph-circle-notch' },
		errored: { label: 'Error', color: 'var(--health-down)', icon: 'ph-x-circle' },
		other: { label: 'Other', color: 'var(--color-neutral-400)', icon: 'ph-question' }
	};

	function act(action: 'stop' | 'start', hashes: string[] | 'all', label: string) {
		void runAction(service.id, action, { label, args: { hashes } });
	}

	async function confirmDelete() {
		const t = deleting;
		deleting = null;
		if (!t) return;
		await runAction(service.id, 'delete', {
			label: `Deleting ${t.name}`,
			sub: deleteFiles ? 'with downloaded files' : 'files kept',
			confirm: true,
			args: { hashes: [t.hash], delete_files: deleteFiles }
		});
	}
</script>

<div class="stats">
	<StatTile
		label="Download"
		value={rate(data.dl_speed).split(' ')[0]}
		unit={rate(data.dl_speed).split(' ')[1]}
	/>
	<StatTile
		label="Upload"
		value={rate(data.up_speed).split(' ')[0]}
		unit={rate(data.up_speed).split(' ')[1]}
	/>
	<StatTile
		label="Free space"
		value={data.free_space != null ? bytes(data.free_space).split(' ')[0] : '–'}
		unit={data.free_space != null ? bytes(data.free_space).split(' ')[1] : ''}
	/>
	<StatTile
		label="Torrents"
		value={data.counts.all}
		trend="{data.counts.downloading} downloading · {data.counts.seeding} seeding"
	/>
</div>

<div class="row actions">
	<label class="alt">
		<Switch
			label="Alternative speed limits"
			checked={data.alt_speed}
			disabled={!canAct}
			onchange={() =>
				void runAction(service.id, 'toggle_alt_speed', {
					label: data.alt_speed ? 'Turning alternative speed off' : 'Turning alternative speed on'
				})}
		/>
		<Icon name="ph-gauge" />Alternative speed limits
	</label>
	<span class="spacer"></span>
	<button
		class="btn btn-secondary btn-sm"
		disabled={!canAct}
		onclick={() => act('stop', 'all', 'Stopping all torrents')}
	>
		<Icon name="ph-pause" />Stop all
	</button>
	<button
		class="btn btn-secondary btn-sm"
		disabled={!canAct}
		onclick={() => act('start', 'all', 'Starting all torrents')}
	>
		<Icon name="ph-play" />Start all
	</button>
</div>

<TimeChart
	title="Speed · last 10 min"
	ranges={false}
	{times}
	height={120}
	series={[
		{ label: 'Down', values: data.history.map((p) => p.dl), format: rate },
		{ label: 'Up', values: data.history.map((p) => p.up), format: rate }
	]}
/>

<div class="row wrap">
	<SearchField
		bind:value={query}
		placeholder="Filter torrents"
		label="Filter torrents"
		width="260px"
	/>
	<SegFilter
		label="Torrent state"
		bind:value={filter}
		options={[
			{ value: 'all', label: 'All', count: data.counts.all },
			{ value: 'downloading', label: 'Downloading', count: data.counts.downloading },
			{ value: 'seeding', label: 'Seeding', count: data.counts.seeding },
			{ value: 'stopped', label: 'Stopped', count: data.counts.stopped },
			{ value: 'errored', label: 'Errored', count: data.counts.errored }
		]}
	/>
</div>

{#if shown.length === 0}
	<EmptyState
		title={filter === 'downloading' ? 'Nothing downloading' : 'No torrents match'}
		detail={filter === 'downloading' ? `${data.counts.seeding} torrents seeding` : undefined}
	>
		{#if filter === 'downloading'}
			<button class="btn btn-ghost btn-sm" onclick={() => (filter = 'seeding')}>Show seeding</button
			>
		{/if}
	</EmptyState>
{:else}
	<ul class="torrents">
		{#each shown as t (t.hash)}
			{@const st = STATE[t.state] ?? STATE.other}
			<li class="t">
				<div class="top">
					<span class="mono name" title={t.name}>{t.name}</span>
					<span class="state" style:color={st.color}>
						<Icon name={st.icon} fill={t.state === 'errored' || t.state === 'stalled'} />{st.label}
					</span>
					{#if canAct}
						{#if t.state === 'stopped' || t.state === 'errored'}
							<button
								class="btn btn-ghost btn-icon"
								aria-label="Start {t.name}"
								title="Start"
								onclick={() => act('start', [t.hash], `Starting ${t.name}`)}
							>
								<Icon name="ph-play" />
							</button>
						{:else}
							<button
								class="btn btn-ghost btn-icon"
								aria-label="Stop {t.name}"
								title="Stop"
								onclick={() => act('stop', [t.hash], `Stopping ${t.name}`)}
							>
								<Icon name="ph-pause" />
							</button>
						{/if}
						<button
							class="btn btn-ghost btn-icon del"
							aria-label="Delete {t.name}"
							title="Delete"
							onclick={() => {
								deleteFiles = false;
								deleting = t;
							}}
						>
							<Icon name="ph-trash" />
						</button>
					{/if}
				</div>
				<div
					class="track"
					role="progressbar"
					aria-label="Progress of {t.name}"
					aria-valuenow={Math.round(t.progress * 100)}
					aria-valuemin={0}
					aria-valuemax={100}
				>
					<div
						class="fill"
						style:width="{t.progress * 100}%"
						style:background={t.progress >= 1 ? 'var(--color-neutral-300)' : st.color}
					></div>
				</div>
				<div class="meta mono num">
					<span>{Math.floor(t.progress * 100)} %</span>
					<span>{bytes(t.downloaded)} / {bytes(t.size)}</span>
					{#if t.dl_speed > 0}<span>↓ {rate(t.dl_speed)}</span>{/if}
					{#if t.up_speed > 0}<span>↑ {rate(t.up_speed)}</span>{/if}
					{#if t.eta_s != null}<span>ETA {duration(t.eta_s)}</span>{/if}
					{#if t.state === 'stalled'}<span>{t.num_seeds} seeds</span>{/if}
					{#if t.category}<span class="cat">{t.category}</span>{/if}
				</div>
			</li>
		{/each}
	</ul>
{/if}

<Dialog
	open={deleting != null}
	title="Delete {deleting?.name}?"
	destructive
	onclose={() => (deleting = null)}
>
	<div class="dlg-title">
		<Icon name="ph-warning-octagon" fill color="var(--health-down)" />Delete torrent?
	</div>
	<div class="target mono">{deleting?.name}</div>
	<label class="checkbox"
		><input type="checkbox" bind:checked={deleteFiles} />Also delete the downloaded files</label
	>
	<p class="caption muted">
		{deleteFiles
			? 'The files are removed from disk. This cannot be undone.'
			: 'The files stay on disk; only the torrent is removed.'}
	</p>
	{#snippet actions()}
		<button class="btn btn-secondary" onclick={() => (deleting = null)}>Cancel</button>
		<button class="btn btn-danger" onclick={confirmDelete}
			><Icon name="ph-trash" />{deleteFiles ? 'Delete with files' : 'Delete torrent'}</button
		>
	{/snippet}
</Dialog>

<style>
	.stats {
		display: grid;
		gap: 10px;
		grid-template-columns: repeat(4, minmax(0, 1fr));
	}
	.actions,
	.wrap {
		gap: 8px;
		flex-wrap: wrap;
	}
	.alt {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		font-size: 13px;
		cursor: pointer;
	}
	.torrents {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.t {
		background: var(--color-surface);
		border-radius: var(--radius-md);
		padding: 10px 12px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.top {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.name {
		flex: 1;
		min-width: 0;
		font-size: 12.5px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.state {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: 11.5px;
		flex: none;
	}
	.del {
		color: var(--health-down);
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
		flex-wrap: wrap;
	}
	.cat {
		margin-left: auto;
	}
	.dlg-title {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 18px;
		font-weight: 500;
	}
	.target {
		padding: 8px 10px;
		border-radius: var(--radius-md);
		background: var(--color-bg);
		font-size: 12.5px;
		word-break: break-all;
	}
	@media (max-width: 900px) {
		.stats {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
	}
</style>
