<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from './Icon.svelte';

	let {
		tone = 'warning',
		text,
		icon,
		children
	}: {
		tone?: 'warning' | 'down' | 'progress' | 'healthy';
		text: string;
		icon?: string;
		children?: Snippet;
	} = $props();
	const defaults = {
		warning: 'ph-warning',
		down: 'ph-wifi-slash',
		progress: 'ph-info',
		healthy: 'ph-check-circle'
	};
</script>

<div
	class="banner"
	role={tone === 'down' ? 'alert' : 'status'}
	style:background="var(--health-{tone}-fill)"
>
	<Icon name={icon ?? defaults[tone]} fill size={18} color="var(--health-{tone})" />
	<span class="text">{text}</span>
	{@render children?.()}
</div>

<style>
	.banner {
		border-radius: var(--radius-md);
		padding: 10px 14px;
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.text {
		flex: 1;
		font-size: 13px;
		color: var(--color-neutral-100);
	}
	.banner :global(.btn) {
		font-size: 12.5px;
		padding: 4px 10px;
		flex: none;
	}
</style>
