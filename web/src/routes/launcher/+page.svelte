<script lang="ts">
	// Launcher: every enabled service as a large tile that opens its web UI (homepage-style),
	// grouped by category. Tiles without a web UI open the service page instead.
	import { resolve } from '$app/paths';
	import type { ServiceView } from '#lib/api/client.js';
	import EmptyState from '#lib/components/EmptyState.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import SearchField from '#lib/components/SearchField.svelte';
	import ServiceIcon from '#lib/components/ServiceIcon.svelte';
	import { CATEGORY_LABEL, CATEGORY_ORDER, HEALTH } from '#lib/health.js';
	import { session } from '#lib/session.svelte.js';

	let query = $state('');
	// The WireGuard base URL is the Quorum agent, not a web UI.
	const webUi = (s: ServiceView) => (s.kind !== 'wireguard' ? s.base_url : null);
	const groups = $derived(
		CATEGORY_ORDER.map((c) => ({
			id: c,
			label: CATEGORY_LABEL[c],
			services: session.services.filter(
				(s) =>
					s.enabled &&
					s.category === c &&
					(!query || s.name.toLowerCase().includes(query.toLowerCase()))
			)
		})).filter((g) => g.services.length > 0)
	);
</script>

<svelte:head><title>Launcher</title></svelte:head>

<div class="page">
	<div class="page-header">
		<h1 class="page-title">Launcher</h1>
		<span class="spacer"></span>
		<SearchField bind:value={query} placeholder="Filter" label="Filter services" />
	</div>
	{#if groups.length === 0}
		<EmptyState icon="ph-squares-four" title={query ? 'No matches' : 'No services yet'} />
	{/if}
	{#each groups as g (g.id)}
		<section>
			<h2 class="overline">{g.label}</h2>
			<div class="tiles">
				{#each g.services as s (s.id)}
					{@const url = webUi(s)}
					<a
						class="card tile"
						href={url ?? resolve('/services/[id]', { id: s.id })}
						target={url ? '_blank' : undefined}
						rel={url ? 'noreferrer noopener' : undefined}
					>
						<ServiceIcon icon={s.icon} hue={s.accent_hue} chroma={s.accent_chroma} size={48} />
						<span class="name">{s.name}</span>
						<span class="host mono faint"
							>{url ? url.replace(/^https?:\/\//, '') : 'No web UI'}</span
						>
						<span
							class="dot"
							style:background={HEALTH[s.health].fg}
							title={HEALTH[s.health].label}
							aria-label={HEALTH[s.health].label}
							role="img"
						></span>
						{#if url}<span class="ext"><Icon name="ph-arrow-square-out" /></span>{/if}
					</a>
				{/each}
			</div>
		</section>
	{/each}
</div>

<style>
	.overline {
		margin: 4px 0 10px;
	}
	.tiles {
		display: grid;
		gap: 12px;
		grid-template-columns: repeat(auto-fill, minmax(176px, 1fr));
	}
	.tile {
		position: relative;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 8px;
		padding: 22px 12px 16px;
		color: inherit;
		text-decoration: none;
		text-align: center;
	}
	.tile:hover {
		background: color-mix(in srgb, var(--color-surface) 92%, var(--color-text));
	}
	/* Keep 4.5:1 contrast on the lighter hover background. */
	.tile:hover .host {
		color: var(--color-neutral-400);
	}
	.name {
		font-size: 14px;
	}
	.host {
		font-size: 11px;
		max-width: 100%;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.dot {
		position: absolute;
		top: 12px;
		left: 12px;
		width: 8px;
		height: 8px;
		border-radius: 50%;
	}
	.ext {
		position: absolute;
		top: 10px;
		right: 10px;
		color: var(--color-neutral-500);
	}
</style>
