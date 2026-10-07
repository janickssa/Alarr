<script lang="ts">
	// Add a service: pick from the catalogue (or a custom health check), then configure.
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import { api, unwrap, type CatalogueEntry } from '#lib/api/client.js';
	import Icon from '#lib/components/Icon.svelte';
	import ServiceIcon from '#lib/components/ServiceIcon.svelte';
	import ServiceForm from '#lib/components/settings/ServiceForm.svelte';
	import { CATEGORY_LABEL, CATEGORY_ORDER } from '#lib/health.js';

	let catalogue = $state<CatalogueEntry[]>([]);
	onMount(async () => {
		catalogue = unwrap(await api.GET('/services/catalogue'));
	});

	const preset = $derived(page.url.searchParams.get('preset'));
	const entry = $derived(catalogue.find((c) => c.preset === preset && c.kind));
	const available = $derived(catalogue.filter((c) => c.kind));
	const later = $derived(catalogue.filter((c) => !c.kind));
</script>

<svelte:head><title>Add service</title></svelte:head>

{#if entry}
	{#key entry.preset}<ServiceForm {entry} />{/key}
{:else if catalogue.length}
	<a class="btn btn-ghost btn-sm back" href={resolve('/settings/services')}
		><Icon name="ph-caret-left" />Services</a
	>
	<h2 class="section-title">Choose what to add</h2>
	{#each CATEGORY_ORDER as cat (cat)}
		{@const items = available.filter((c) => c.category === cat)}
		{#if items.length}
			<section>
				<h3 class="overline">{CATEGORY_LABEL[cat]}</h3>
				<div class="picker">
					{#each items as c (c.preset)}
						<a class="pick" href="{resolve('/settings/services/new')}?preset={c.preset}">
							<ServiceIcon icon={c.icon} hue={c.accent_hue} chroma={c.accent_chroma} />
							<span class="txt"
								><span>{c.name}</span><span class="caption faint"
									>{c.kind === 'health_check' ? 'Health check' : 'Full integration'}</span
								></span
							>
						</a>
					{/each}
				</div>
			</section>
		{/if}
	{/each}
	{#if later.length}
		<section>
			<h3 class="overline">Coming in later phases</h3>
			<p class="caption muted">
				Full integrations for {later.map((c) => c.name).join(', ')}. Until then, add them as a
				custom health check.
			</p>
		</section>
	{/if}
{/if}

<style>
	.back {
		align-self: flex-start;
	}
	.overline {
		margin: 8px 0;
	}
	.picker {
		display: grid;
		gap: 10px;
		grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
	}
	.pick {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 12px;
		border-radius: var(--radius-md);
		background: var(--color-surface);
		color: var(--color-text);
		text-decoration: none;
	}
	.pick:hover {
		color: var(--color-text);
		box-shadow: 0 0 0 1px var(--color-accent);
	}
	.txt {
		display: flex;
		flex-direction: column;
		font-size: 14px;
	}
</style>
