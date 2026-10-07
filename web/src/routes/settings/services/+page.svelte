<script lang="ts">
	// Manage services: enable/disable toggles, configure, add.
	import { resolve } from '$app/paths';
	import { api, errorMessage, unwrap, type ServiceView } from '#lib/api/client.js';
	import EmptyState from '#lib/components/EmptyState.svelte';
	import HealthBadge from '#lib/components/HealthBadge.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import ServiceIcon from '#lib/components/ServiceIcon.svelte';
	import Switch from '#lib/components/Switch.svelte';
	import { guestId } from '#lib/format.js';
	import { CATEGORY_LABEL } from '#lib/health.js';
	import { session } from '#lib/session.svelte.js';
	import { toasts } from '#lib/toasts.svelte.js';

	async function setEnabled(s: ServiceView, on: boolean) {
		try {
			unwrap(
				on
					? await api.POST('/services/{id}/enable', { params: { path: { id: s.id } } })
					: await api.POST('/services/{id}/disable', { params: { path: { id: s.id } } })
			);
			await session.loadServices();
		} catch (e) {
			toasts.push({
				kind: 'error',
				title: `Couldn't ${on ? 'enable' : 'disable'} ${s.name}`,
				sub: errorMessage(e)
			});
			await session.loadServices();
		}
	}
</script>

<svelte:head><title>Manage services</title></svelte:head>

<div class="row">
	<p class="muted caption">
		Turn services on or off. Off services are not polled and leave no empty slot on Home.
	</p>
	<span class="spacer"></span>
	<a class="btn btn-primary" href={resolve('/settings/services/new')}
		><Icon name="ph-plus" />Add service</a
	>
</div>

{#if session.services.length === 0}
	<EmptyState
		icon="ph-plus-circle"
		title="No services yet"
		detail="Start with your Proxmox cluster, then add health checks."
	/>
{:else}
	<div class="card list">
		<table class="table">
			<thead>
				<tr
					><th>Service</th><th>Category</th><th>Status</th><th>Container</th><th>On</th><th
					></th></tr
				>
			</thead>
			<tbody>
				{#each session.services as s (s.id)}
					<tr>
						<td>
							<span class="name">
								<ServiceIcon icon={s.icon} hue={s.accent_hue} chroma={s.accent_chroma} size={24} />
								{s.name}<span class="mono faint">{s.id}</span>
							</span>
						</td>
						<td class="muted">{CATEGORY_LABEL[s.category]}</td>
						<td><HealthBadge health={s.health} small /></td>
						<td class="mono muted"
							>{s.linked_guest ? guestId(s.linked_guest.guest_type, s.linked_guest.vmid) : '–'}</td
						>
						<td
							><Switch
								label="{s.name} enabled"
								checked={s.enabled}
								onchange={(v) => setEnabled(s, v)}
							/></td
						>
						<td class="right">
							<a
								class="btn btn-ghost btn-sm"
								href={resolve('/settings/services/[id]', { id: s.id })}
								><Icon name="ph-gear-six" />Configure</a
							>
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
{/if}

<style>
	.list {
		padding: 4px 8px;
		overflow-x: auto;
	}
	.name {
		display: inline-flex;
		align-items: center;
		gap: 10px;
		white-space: nowrap;
	}
	.name .mono {
		font-size: 11.5px;
	}
</style>
