<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from './Icon.svelte';

	let {
		title,
		detail,
		onretry,
		icon = 'ph-plugs',
		children
	}: {
		title: string;
		detail?: string;
		onretry?: () => void;
		icon?: string;
		children?: Snippet;
	} = $props();
</script>

<div class="state" role="alert">
	<div class="ring"><Icon name={icon} fill size={24} color="var(--health-down)" /></div>
	<span class="title">{title}</span>
	{#if detail}<span class="detail mono">{detail}</span>{/if}
	<div class="row">
		{#if onretry}<button class="btn btn-primary btn-sm" onclick={onretry}>Retry</button>{/if}
		{@render children?.()}
	</div>
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
		background: var(--health-down-fill);
		display: grid;
		place-items: center;
	}
	.title {
		font-size: 14px;
	}
	.detail {
		font-size: 11.5px;
		color: var(--color-neutral-400);
	}
	.row {
		display: flex;
		gap: 6px;
	}
</style>
