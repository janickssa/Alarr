<script lang="ts">
	// Shared service page header: icon, name, health, linked container with power menu,
	// "Open web UI" and (owner) settings.
	import { resolve } from '$app/paths';
	import type { Snippet } from 'svelte';
	import type { Guest, ServiceView } from '#lib/api/client.js';
	import { guestId } from '#lib/format.js';
	import { session } from '#lib/session.svelte.js';
	import HealthBadge from '../HealthBadge.svelte';
	import Icon from '../Icon.svelte';
	import PowerMenu from '../PowerMenu.svelte';
	import RelativeTime from '../RelativeTime.svelte';
	import ServiceIcon from '../ServiceIcon.svelte';

	let {
		service,
		guest,
		actions
	}: { service: ServiceView; guest: Guest | null; actions?: Snippet } = $props();
</script>

<header class="svc-head">
	<ServiceIcon
		icon={service.icon}
		hue={service.accent_hue}
		chroma={service.accent_chroma}
		size={44}
	/>
	<div class="titles">
		<div class="row">
			<h1 class="page-title">{service.name}</h1>
			<HealthBadge health={service.health} />
		</div>
		<span class="caption muted">
			{#if service.health_reason}{service.health_reason} ·
			{/if}
			{#if service.version}<span class="mono">v{service.version}</span> ·
			{/if}
			{#if service.updated_at}<RelativeTime value={service.updated_at} prefix="Updated " />{/if}
		</span>
	</div>
	<span class="spacer"></span>
	{#if service.linked_guest}
		<div class="guest-chip">
			<Icon name="ph-cube" color="var(--color-neutral-400)" />
			<span class="mono"
				>{guestId(service.linked_guest.guest_type, service.linked_guest.vmid)} · {service
					.linked_guest.name} · {service.linked_guest.node}</span
			>
			<HealthBadge
				health={service.linked_guest.status === 'running' ? 'healthy' : 'disabled'}
				label={service.linked_guest.status}
				small
			/>
			{#if guest && session.proxmox}
				<PowerMenu {guest} linked={[service.name]} disabled={!session.canAct(session.proxmox.id)} />
			{/if}
		</div>
	{/if}
	{#if session.isOwner}
		<a
			class="btn btn-secondary btn-icon"
			href={resolve('/settings/services/[id]', { id: service.id })}
			aria-label="Settings for {service.name}"
			title="Settings"><Icon name="ph-gear-six" /></a
		>
	{/if}
	{#if service.base_url}
		<a class="btn btn-primary" href={service.base_url} target="_blank" rel="noreferrer noopener">
			<Icon name="ph-arrow-square-out" />Open web UI
		</a>
	{/if}
</header>
{#if actions}<div class="actions row">{@render actions()}</div>{/if}

<style>
	.svc-head {
		display: flex;
		align-items: center;
		gap: 14px;
		flex-wrap: wrap;
	}
	.titles {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.titles .row {
		gap: 12px;
	}
	.guest-chip {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 4px 4px 4px 10px;
		border-radius: var(--radius-md);
		background: var(--color-surface);
		font-size: 12.5px;
	}
	.actions {
		gap: 8px;
		flex-wrap: wrap;
	}
</style>
