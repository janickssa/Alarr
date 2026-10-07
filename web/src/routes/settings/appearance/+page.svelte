<script lang="ts">
	// Home layout (order and visibility) and theme notes.
	import { onMount } from 'svelte';
	import { api, errorMessage, unwrap } from '#lib/api/client.js';
	import Icon from '#lib/components/Icon.svelte';
	import ServiceIcon from '#lib/components/ServiceIcon.svelte';
	import Switch from '#lib/components/Switch.svelte';
	import { session } from '#lib/session.svelte.js';
	import { toasts } from '#lib/toasts.svelte.js';

	let order = $state<string[]>([]);
	let hidden = $state<string[]>([]);
	let loaded = $state(false);

	onMount(async () => {
		const s = unwrap(await api.GET('/settings')).settings;
		const ids = session.services.filter((x) => x.enabled).map((x) => x.id);
		order = [
			...s.home_order.filter((id) => ids.includes(id)),
			...ids.filter((id) => !s.home_order.includes(id))
		];
		hidden = s.home_hidden;
		loaded = true;
	});

	const byId = $derived(Object.fromEntries(session.services.map((s) => [s.id, s])));

	function move(i: number, d: number) {
		const j = i + d;
		if (j < 0 || j >= order.length) return;
		[order[i], order[j]] = [order[j], order[i]];
	}

	async function save() {
		try {
			unwrap(await api.PUT('/settings', { body: { home_order: order, home_hidden: hidden } }));
			toasts.push({ kind: 'success', title: 'Home layout saved' });
		} catch (e) {
			toasts.push({ kind: 'error', title: "Couldn't save", sub: errorMessage(e) });
		}
	}
</script>

<svelte:head><title>Appearance</title></svelte:head>

<section class="card narrow">
	<h2 class="section-title">Home cards</h2>
	<p class="caption muted">Order and visibility of the cards on Home. The same as Edit on Home.</p>
	{#if loaded}
		<ul>
			{#each order as id, i (id)}
				{@const s = byId[id]}
				{#if s}
					<li>
						<ServiceIcon icon={s.icon} hue={s.accent_hue} chroma={s.accent_chroma} size={24} />
						<span class="spacer">{s.name}</span>
						<button
							class="btn btn-ghost btn-icon"
							aria-label="Move {s.name} up"
							disabled={i === 0}
							onclick={() => move(i, -1)}><Icon name="ph-arrow-up" /></button
						>
						<button
							class="btn btn-ghost btn-icon"
							aria-label="Move {s.name} down"
							disabled={i === order.length - 1}
							onclick={() => move(i, 1)}><Icon name="ph-arrow-down" /></button
						>
						<Switch
							label="Show {s.name} on Home"
							checked={!hidden.includes(id)}
							onchange={(v) => (hidden = v ? hidden.filter((h) => h !== id) : [...hidden, id])}
						/>
					</li>
				{/if}
			{/each}
		</ul>
		<div class="row">
			<span class="spacer"></span><button class="btn btn-primary" onclick={save}>Save layout</button
			>
		</div>
	{/if}
</section>

<section class="card narrow">
	<h2 class="section-title">Theme</h2>
	<p class="caption muted">Nocturne, dark. A light theme is not designed yet.</p>
</section>

<style>
	.narrow {
		max-width: 640px;
	}
	ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	li {
		display: flex;
		align-items: center;
		gap: 8px;
		min-height: 36px;
	}
</style>
