<script lang="ts">
	// Status badge: tinted fill + filled icon + label. Used for standalone status.
	import type { Health } from '#lib/api/client.js';
	import { HEALTH } from '#lib/health.js';
	import Icon from './Icon.svelte';

	let {
		health,
		label,
		small = false
	}: { health: Health; label?: string; small?: boolean } = $props();
	const meta = $derived(HEALTH[health] ?? HEALTH.unknown);
</script>

<span
	class="badge"
	class:small
	style:background={meta.fill}
	style:color={meta.fg}
	title={label && label !== meta.label ? meta.label : undefined}
>
	<Icon
		name={meta.icon}
		fill={health !== 'in_progress'}
		spin={health === 'in_progress'}
		size={small ? 12 : 14}
	/>{label ?? meta.label}
</span>

<style>
	.badge {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 2px 8px 2px 6px;
		border-radius: 6px;
		font-size: 12px;
		font-weight: 500;
		white-space: nowrap;
		line-height: 1.5;
	}
	.small {
		font-size: 11px;
		padding: 1px 7px 1px 5px;
	}
</style>
