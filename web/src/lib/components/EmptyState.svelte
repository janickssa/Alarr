<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from './Icon.svelte';

	let {
		icon = 'ph-tray',
		title,
		detail,
		children
	}: { icon?: string; title: string; detail?: string; children?: Snippet } = $props();
</script>

<div class="state">
	<div class="ring"><Icon name={icon} size={24} color="var(--color-accent-300)" /></div>
	<span class="title">{title}</span>
	{#if detail}<span class="detail">{detail}</span>{/if}
	{@render children?.()}
</div>

<style>
	.state {
		background: var(--color-surface);
		border-radius: var(--radius-md);
		padding: 24px 14px;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 8px;
		text-align: center;
	}
	.ring {
		width: 52px;
		height: 52px;
		border-radius: 50%;
		box-shadow:
			0 0 0 1px var(--color-neutral-700),
			0 0 0 8px color-mix(in srgb, var(--color-accent) 6%, transparent);
		display: grid;
		place-items: center;
	}
	.title {
		font-size: 14px;
	}
	.detail {
		font-size: 12px;
		color: var(--color-neutral-400);
	}
</style>
