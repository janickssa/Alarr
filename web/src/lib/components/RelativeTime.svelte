<script lang="ts">
	// "2 min ago", with the absolute date and time on hover. Updates every 10 s.
	import { dateTime, relative } from '#lib/format.js';
	import { onMount } from 'svelte';

	let { value, prefix = '' }: { value: string | null | undefined; prefix?: string } = $props();
	let now = $state(new Date());
	onMount(() => {
		const t = setInterval(() => (now = new Date()), 10_000);
		return () => clearInterval(t);
	});
</script>

{#if value}
	<time datetime={value} title={dateTime(value)}>{prefix}{relative(value, now)}</time>
{:else}
	<span>–</span>
{/if}
