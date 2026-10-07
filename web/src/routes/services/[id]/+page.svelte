<script lang="ts">
	// Service page: shared header (health, linked container with power menu, Open web UI),
	// then the module's own content.
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import {
		api,
		errorMessage,
		unwrap,
		type Guest,
		type ServiceSnapshot,
		type ServiceView
	} from '#lib/api/client.js';
	import ErrorState from '#lib/components/ErrorState.svelte';
	import ArrBody from '#lib/components/service/ArrBody.svelte';
	import AuthentikBody from '#lib/components/service/AuthentikBody.svelte';
	import BazarrBody from '#lib/components/service/BazarrBody.svelte';
	import HealthCheckBody from '#lib/components/service/HealthCheckBody.svelte';
	import JellyfinBody from '#lib/components/service/JellyfinBody.svelte';
	import NavidromeBody from '#lib/components/service/NavidromeBody.svelte';
	import NextcloudBody from '#lib/components/service/NextcloudBody.svelte';
	import NpmBody from '#lib/components/service/NpmBody.svelte';
	import ProwlarrBody from '#lib/components/service/ProwlarrBody.svelte';
	import QbittorrentBody from '#lib/components/service/QbittorrentBody.svelte';
	import SeerrBody from '#lib/components/service/SeerrBody.svelte';
	import ServiceHeader from '#lib/components/service/ServiceHeader.svelte';
	import TechnitiumBody from '#lib/components/service/TechnitiumBody.svelte';
	import WireguardBody from '#lib/components/service/WireguardBody.svelte';
	import SkeletonCard from '#lib/components/SkeletonCard.svelte';
	import EmptyState from '#lib/components/EmptyState.svelte';
	import { refetchOnLive } from '#lib/live.svelte.js';
	import { session } from '#lib/session.svelte.js';

	const id = $derived(page.params.id ?? '');
	let service = $state<ServiceView | null>(null);
	let snapshot = $state<ServiceSnapshot | null>(null);
	let guest = $state<Guest | null>(null);
	let error = $state('');
	let tick = $state(0);

	async function load(sid: string) {
		try {
			const s = unwrap(await api.GET('/services/{id}', { params: { path: { id: sid } } }));
			if (s.kind === 'proxmox') {
				await goto(resolve('/proxmox'), { replace: true });
				return;
			}
			const snap = await api.GET('/services/{id}/snapshot', { params: { path: { id: sid } } });
			service = s;
			snapshot = snap.data ?? null;
			error = '';
			tick++;
			if (s.linked_guest && session.proxmox) {
				const g = await api.GET('/proxmox/guests/{vmid}', {
					params: { path: { vmid: s.linked_guest.vmid } }
				});
				guest = g.data?.guest ?? null;
			} else guest = null;
		} catch (e) {
			error = errorMessage(e);
		}
	}

	$effect(() => {
		service = null;
		snapshot = null;
		void load(id);
	});
	refetchOnLive(
		() => [id, ...(session.proxmox ? [session.proxmox.id] : [])],
		() => load(id),
		{ minInterval: 2000 }
	);

	const data = $derived(snapshot?.data);
</script>

<svelte:head><title>{service?.name ?? 'Service'}</title></svelte:head>

<div class="page">
	{#if error && !service}
		<ErrorState title="Can't load this service" detail={error} onretry={() => load(id)} />
	{:else if !service}
		<div class="skeleton" style="height:44px;width:280px"></div>
		<div class="grid">
			{#each Array(4), i (i)}<SkeletonCard />{/each}
		</div>
	{:else}
		<ServiceHeader {service} {guest} />
		{#if !service.enabled}
			<EmptyState
				icon="ph-minus-circle"
				title="{service.name} is turned off"
				detail="Turn it on in Settings to see its data."
			/>
		{:else if service.kind === 'health_check'}
			<HealthCheckBody {service} {snapshot} {tick} />
		{:else if !data || data.type === 'none'}
			{#if service.health === 'down'}
				<ErrorState
					title="Can't reach {service.name}"
					detail={service.health_reason ?? service.base_url ?? ''}
					onretry={() => load(id)}
				/>
			{:else}
				<div class="grid">
					{#each Array(4), i (i)}<SkeletonCard />{/each}
				</div>
			{/if}
		{:else}
			{#if snapshot?.stale}
				<div class="stale caption">
					Showing data from the last successful check. {service.health_reason}
				</div>
			{/if}
			{#if data.type === 'qbittorrent'}
				<QbittorrentBody {service} data={data.value} />
			{:else if data.type === 'arr'}
				<ArrBody {service} data={data.value} />
			{:else if data.type === 'jellyfin'}
				<JellyfinBody {service} data={data.value} />
			{:else if data.type === 'seerr'}
				<SeerrBody {service} data={data.value} />
			{:else if data.type === 'prowlarr'}
				<ProwlarrBody {service} data={data.value} />
			{:else if data.type === 'bazarr'}
				<BazarrBody {service} data={data.value} />
			{:else if data.type === 'navidrome'}
				<NavidromeBody {service} data={data.value} />
			{:else if data.type === 'technitium'}
				<TechnitiumBody {service} data={data.value} />
			{:else if data.type === 'npm'}
				<NpmBody data={data.value} />
			{:else if data.type === 'authentik'}
				<AuthentikBody data={data.value} />
			{:else if data.type === 'nextcloud'}
				<NextcloudBody data={data.value} />
			{:else if data.type === 'wireguard'}
				<WireguardBody data={data.value} />
			{/if}
		{/if}
	{/if}
</div>

<style>
	.stale {
		padding: 8px 12px;
		border-radius: var(--radius-md);
		background: var(--health-warning-fill);
		color: var(--color-neutral-100);
	}
</style>
