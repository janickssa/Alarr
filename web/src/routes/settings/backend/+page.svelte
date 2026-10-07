<script lang="ts">
	import { onMount } from 'svelte';
	import { api, unwrap, type Schemas } from '#lib/api/client.js';
	import RelativeTime from '#lib/components/RelativeTime.svelte';
	import { dateTime } from '#lib/format.js';
	import { connection } from '#lib/connection.svelte.js';

	let info = $state<Schemas['BackendInfo'] | null>(null);
	onMount(async () => {
		info = unwrap(await api.GET('/settings')).backend;
	});
</script>

<svelte:head><title>Backend</title></svelte:head>

{#if info}
	<section class="card narrow">
		<h2 class="section-title">Backend</h2>
		<dl class="facts">
			<dt>Server</dt>
			<dd>
				{info.name} <span class="mono">{info.version}</span>{#if info.demo}
					· <span class="tag tag-accent">demo mode</span>{/if}
			</dd>
			<dt>Public URL</dt>
			<dd class="mono">{info.public_url ?? 'not set'}</dd>
			<dt>Started</dt>
			<dd title={dateTime(info.started_at)}><RelativeTime value={info.started_at} /></dd>
			<dt>Pollers</dt>
			<dd>{info.pollers} running</dd>
			<dt>Live updates</dt>
			<dd>{connection.live === 'open' ? 'Connected' : 'Reconnecting'}</dd>
			<dt>API</dt>
			<dd><a href="/api/v1/openapi.json" target="_blank" rel="noreferrer">OpenAPI document</a></dd>
		</dl>
	</section>
{/if}

<style>
	.narrow {
		max-width: 640px;
	}
	.facts {
		display: grid;
		grid-template-columns: 120px 1fr;
		gap: 8px 12px;
		margin: 0;
		font-size: 14px;
	}
	dt {
		color: var(--color-neutral-400);
	}
	dd {
		margin: 0;
	}
</style>
