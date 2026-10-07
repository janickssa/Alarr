<script lang="ts">
	// Component gallery: compare against design/01 Identity & Design System.dc.html.
	import type { Guest, Health, RrdRange, ServiceView } from '#lib/api/client.js';
	import Banner from '#lib/components/Banner.svelte';
	import EmptyState from '#lib/components/EmptyState.svelte';
	import ErrorState from '#lib/components/ErrorState.svelte';
	import HealthBadge from '#lib/components/HealthBadge.svelte';
	import HealthCheckCard from '#lib/components/HealthCheckCard.svelte';
	import HealthDot from '#lib/components/HealthDot.svelte';
	import LargeCard from '#lib/components/LargeCard.svelte';
	import Logo from '#lib/components/Logo.svelte';
	import NowPlayingRow from '#lib/components/media/NowPlayingRow.svelte';
	import PosterCard from '#lib/components/media/PosterCard.svelte';
	import QueueRow from '#lib/components/media/QueueRow.svelte';
	import RequestTracker from '#lib/components/media/RequestTracker.svelte';
	import PowerMenu from '#lib/components/PowerMenu.svelte';
	import ResourceBar from '#lib/components/ResourceBar.svelte';
	import SearchField from '#lib/components/SearchField.svelte';
	import SegFilter from '#lib/components/SegFilter.svelte';
	import ServiceCard from '#lib/components/ServiceCard.svelte';
	import ServiceIcon from '#lib/components/ServiceIcon.svelte';
	import ServiceRow from '#lib/components/ServiceRow.svelte';
	import SkeletonCard from '#lib/components/SkeletonCard.svelte';
	import Sparkline from '#lib/components/Sparkline.svelte';
	import StatTile from '#lib/components/StatTile.svelte';
	import TimeChart from '#lib/components/TimeChart.svelte';
	import UptimeStrip from '#lib/components/UptimeStrip.svelte';
	import { power } from '#lib/power.svelte.js';
	import { toasts } from '#lib/toasts.svelte.js';
	import { rate } from '#lib/format.js';

	const healths: Health[] = ['healthy', 'warning', 'down', 'in_progress', 'disabled', 'unknown'];
	const series = (n: number, base: number, amp: number, seed: number) =>
		Array.from(
			{ length: n },
			(_, i) =>
				base +
				amp *
					(Math.sin(i * 0.55 + seed) * 0.5 +
						Math.sin(i * 1.7 + seed * 2) * 0.3 +
						Math.cos(i * 0.21) * 0.2)
		);
	const now = Math.floor(Date.now() / 1000);
	const times = Array.from({ length: 60 }, (_, i) => now - (59 - i) * 1440);
	let range = $state<RrdRange>('day');
	let filter = $state('all');
	let search = $state('');

	const svc = (over: Partial<ServiceView>): ServiceView =>
		({
			id: 'jellyfin',
			kind: 'health_check',
			name: 'Jellyfin',
			category: 'media',
			icon: 'film-strip',
			accent_hue: 305,
			accent_chroma: 0.045,
			level: 'full',
			enabled: true,
			sort_order: 0,
			settings: {},
			secrets: {},
			health: 'healthy',
			response_ms: 42,
			updated_at: new Date(Date.now() - 12000).toISOString(),
			linked_guest: {
				vmid: 108,
				guest_type: 'lxc',
				name: 'jellyfin',
				node: 'pve-01',
				status: 'running'
			},
			...over
		}) as ServiceView;
	const guest: Guest = {
		vmid: 123,
		name: 'sonarr',
		node: 'pve-02',
		guest_type: 'lxc',
		status: 'running',
		tags: [],
		template: false,
		linked_services: ['sonarr']
	} as unknown as Guest;
	const uptime = Array.from({ length: 48 }, (_, i) => ({
		start: new Date((now - (47 - i) * 1800) * 1000).toISOString(),
		health: (i === 31 ? 'down' : i === 30 || i === 12 ? 'warning' : 'healthy') as Health
	}));
	const accents = [
		['Rose', 10],
		['Clay', 45],
		['Sand', 75],
		['Moss', 115],
		['Teal', 190],
		['Steel', 222],
		['Iris', 268],
		['Orchid', 305],
		['Mauve', 335]
	] as const;
</script>

<svelte:head><title>Components</title></svelte:head>

<div class="page gallery">
	<h1 class="page-title">Component gallery</h1>
	<p class="muted">
		Compare with <span class="mono">design/01 Identity &amp; Design System.dc.html</span> (batch 1).
	</p>

	<section>
		<h2 class="overline">1a Lockup</h2>
		<Logo />
	</section>

	<section>
		<h2 class="overline">1b Health and accents</h2>
		<div class="row wrap">
			{#each healths as h (h)}<HealthBadge health={h} />{/each}
		</div>
		<div class="row wrap">
			{#each healths as h (h)}<HealthDot health={h} />{/each}
		</div>
		<div class="row wrap">
			{#each accents as [n, h] (n)}<span class="row"><ServiceIcon icon="ph-cube" hue={h} />{n}</span
				>{/each}<span class="row"
				><ServiceIcon icon="ph-puzzle-piece" hue={265} chroma={0.015} />Slate</span
			>
		</div>
	</section>

	<section>
		<h2 class="overline">1c Type scale</h2>
		<div style="font:var(--text-large-title)">All 18 healthy</div>
		<div style="font:var(--text-title)">Proxmox cluster</div>
		<div style="font:var(--text-stat)" class="num">38.2 MB/s</div>
		<div style="font:var(--text-headline)">Now playing</div>
		<div style="font:var(--text-body)">3 requests waiting for approval</div>
		<div style="font:var(--text-caption)">Updated 2 min ago</div>
		<div class="overline">Network and security</div>
		<div style="font:var(--text-mono)">CT 123 · 10.0.20.123 · pve-02</div>
	</section>

	<section>
		<h2 class="overline">1e Stat tiles, resource bars, charts</h2>
		<div class="stats">
			<StatTile
				label="Download"
				value="38.2"
				unit="MB/s"
				trend="+12 % vs 1 h"
				trendIcon="ph-arrow-up-right"
			/>
			<StatTile label="Active streams" value="2" trend="1 transcode" />
			<StatTile
				label="DNS blocked"
				value="12"
				unit="%"
				trend="−2 % vs yesterday"
				trendIcon="ph-arrow-down-right"
			/>
			<StatTile
				label="Free on /media"
				value="1.8"
				unit="TB"
				trend="−42 GB today"
				trendIcon="ph-arrow-down-right"
			/>
		</div>
		<div class="bars">
			<ResourceBar label="CPU · 8 cores" fraction={0.42} value="42 %" />
			<ResourceBar label="RAM" fraction={0.85} value="27.1 / 32 GB · 85 %" />
			<ResourceBar label="local-zfs" fraction={0.94} value="3.4 / 3.6 TB · 94 %" />
		</div>
		<div class="row wrap top">
			<div class="card" style="width:200px">
				<div class="row caption">
					<span class="muted">CPU · 24 h</span><span class="spacer"></span><span class="mono"
						>18 %</span
					>
				</div>
				<Sparkline values={series(32, 16, 10, 1)} />
			</div>
			<div style="width:200px"><UptimeStrip buckets={uptime} uptime={99.3} /></div>
			<div style="flex:1;min-width:320px">
				<TimeChart
					title="Network · pve-01"
					bind:range
					{times}
					series={[
						{ label: 'In', values: series(60, 55, 40, 0.4).map((v) => v * 1e6), format: rate },
						{ label: 'Out', values: series(60, 18, 10, 2).map((v) => v * 1e6), format: rate }
					]}
				/>
			</div>
		</div>
	</section>

	<section>
		<h2 class="overline">1f Service cards</h2>
		<div class="card list">
			<ServiceRow
				name="Jellyfin"
				sub="2 streams · CT 108"
				icon="film-strip"
				hue={305}
				health="healthy"
				href="#x"
			/>
			<ServiceRow
				name="Radarr"
				sub="Unreachable for 4 min · CT 124"
				icon="film-slate"
				hue={75}
				health="down"
				href="#x"
			/>
			<ServiceRow
				name="Navidrome"
				sub="Turned off in settings"
				icon="music-notes"
				hue={10}
				health="disabled"
				href="#x"
			/>
		</div>
		<div class="cards">
			<ServiceCard service={svc({})} headline="2 streams" detail="1 transcode · 38 Mbit/s" />
			<ServiceCard
				service={svc({
					name: 'Sonarr',
					icon: 'television-simple',
					accent_hue: 222,
					health: 'warning'
				})}
				headline="3 in queue"
				detail="Indexer unavailable · 6 h"
			/>
			<HealthCheckCard
				service={svc({
					name: 'BookStack',
					icon: 'books',
					accent_hue: 115,
					base_url: 'https://books.example.lan'
				})}
				uptime7d="99.9 % · 7 d"
			/>
			<LargeCard
				service={svc({ name: 'qBittorrent', icon: 'arrows-down-up', accent_hue: 190 })}
				stats={[
					{ label: 'Down', value: '38.2', unit: 'MB/s' },
					{ label: 'Up', value: '2.4', unit: 'MB/s' },
					{ label: 'Free', value: '1.8', unit: 'TB' }
				]}
				chart={series(40, 22, 12, 3)}
				chartLabel="Last 10 min"
			>
				{#snippet badge()}<HealthBadge health="in_progress" label="4 downloading" small />{/snippet}
			</LargeCard>
		</div>
	</section>

	<section>
		<h2 class="overline">1g Media</h2>
		<div class="row top wrap">
			<PosterCard title="Glass Meridian" meta="Movie · 2025" isNew />
			<PosterCard title="Saltwater Kings" meta="S01 · 8 ep" colors={['#2f4a52', '#1b1d2b']} />
			<PosterCard title="Harbor Lights" meta="Movie · 2024" colors={['#4f3d33', '#1b1d2b']} />
			<div class="col">
				<NowPlayingRow
					title="The Long Orbit · S02E05"
					detail="mara · Living room TV · 1080p HEVC → H.264 · 12.4 Mbit/s"
					progress={0.58}
					position="31:12 / 54:00"
					transcode
					reason="audio codec not supported (TrueHD)"
				/>
				<QueueRow
					name="Glass.Meridian.2025.2160p.WEB-DL.DDP5.1"
					state="Downloading"
					progress={0.62}
					size="11.4 / 18.4 GB"
					speed="24.1 MB/s"
					eta="5 min"
					source="radarr"
				/>
			</div>
		</div>
		<RequestTracker
			title="Glass Meridian (2025)"
			meta="requested by leo · 2 h ago"
			done={3}
			subs={['Seerr', 'auto', 'Radarr · 2160p', '62 % · ETA 5 min']}
		/>
		<RequestTracker
			title="Saltwater Kings · S01"
			meta="requested by mara · 1 d ago"
			done={2}
			stuck
			subs={['Seerr', 'janick', 'No seeders · 6 h']}
		/>
	</section>

	<section>
		<h2 class="overline">1h Power, confirmations, toasts, banners</h2>
		<div class="row wrap">
			<PowerMenu {guest} />
			<button
				class="btn btn-secondary"
				onclick={() => power.confirm(guest, 'reboot', ['Sonarr (healthy)'])}
				>Confirm · normal</button
			>
			<button class="btn btn-secondary" onclick={() => power.confirm(guest, 'stop')}
				>Confirm · destructive</button
			>
			<button
				class="btn btn-secondary"
				onclick={() => {
					const id = toasts.push({
						kind: 'progress',
						title: 'Rebooting sonarr',
						sub: 'CT 123 · pve-02',
						mono: true
					});
					setTimeout(
						() =>
							toasts.update(id, {
								kind: 'success',
								title: 'sonarr rebooted in 18 s',
								sub: 'CT 123 · UPID:pve-02:0003A1F2'
							}),
						2000
					);
				}}>Toast: progress → success</button
			>
			<button
				class="btn btn-secondary"
				onclick={() =>
					toasts.push({
						kind: 'error',
						title: 'Start failed: jellyfin-public',
						sub: "storage 'tank' is not online",
						mono: true
					})}>Toast: error</button
			>
		</div>
		<Banner tone="warning" text="Sonarr reports: Indexer Prowlarr has been unavailable for 6 hours"
			><button class="btn btn-secondary">Open Prowlarr</button></Banner
		>
		<Banner tone="progress" icon="ph-pause-circle" text="DNS blocking paused · resumes in 24:13"
			><button class="btn btn-secondary">Resume now</button></Banner
		>
		<Banner
			tone="down"
			text="Can't reach the backend. Showing data from 14:02. Are you on Tailscale?"
			><button class="btn btn-secondary">Retry</button></Banner
		>
		<div class="row wrap">
			<button class="btn btn-primary">Primary</button>
			<button class="btn btn-secondary">Secondary</button>
			<button class="btn btn-ghost">Ghost</button>
			<button class="btn btn-danger">Danger</button>
			<button class="btn btn-primary" disabled>Disabled</button>
		</div>
	</section>

	<section>
		<h2 class="overline">1i Filters, search, states</h2>
		<div class="row wrap">
			<SearchField
				bind:value={search}
				shortcut
				placeholder="Search services, containers, titles…"
			/>
			<SegFilter
				label="Torrents"
				bind:value={filter}
				options={[
					{ value: 'all', label: 'All', count: 24 },
					{ value: 'dl', label: 'Downloading', count: 4 },
					{ value: 'seed', label: 'Seeding', count: 17 },
					{ value: 'err', label: 'Errored', count: 1 }
				]}
			/>
		</div>
		<div class="row wrap">
			<span class="tag tag-outline">Node: pve-01</span><span class="tag tag-neutral">Type: CT</span
			><span class="tag tag-neutral">Tag: media</span><button class="btn btn-ghost btn-sm"
				>Clear</button
			>
		</div>
		<div class="cards">
			<SkeletonCard />
			<EmptyState title="Nothing downloading" detail="17 torrents seeding"
				><button class="btn btn-ghost btn-sm">Show seeding</button></EmptyState
			>
			<ErrorState
				title="Can't reach Radarr"
				detail="10.0.20.124:7878 · timeout 5 s"
				onretry={() => {}}><button class="btn btn-secondary btn-sm">Power menu</button></ErrorState
			>
		</div>
	</section>
</div>

<style>
	.gallery section {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding-bottom: 12px;
	}
	.wrap {
		flex-wrap: wrap;
		gap: 10px;
	}
	.top {
		align-items: flex-start;
	}
	.stats,
	.cards {
		display: grid;
		gap: 12px;
		grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
	}
	.bars {
		display: flex;
		flex-direction: column;
		gap: 12px;
		max-width: 640px;
	}
	.list {
		padding: 0;
		gap: 0;
		max-width: 640px;
	}
	.col {
		display: flex;
		flex-direction: column;
		gap: 10px;
		flex: 1;
		min-width: 320px;
	}
</style>
