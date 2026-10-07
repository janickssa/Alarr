<script lang="ts">
	// Filled status icon with an accessible label (for compact card headers).
	import type { Health } from '#lib/api/client.js';
	import { HEALTH } from '#lib/health.js';
	import Icon from './Icon.svelte';

	let { health, size = 16 }: { health: Health; size?: number } = $props();
	const meta = $derived(HEALTH[health] ?? HEALTH.unknown);
</script>

<Icon
	name={meta.icon}
	fill={health !== 'in_progress'}
	spin={health === 'in_progress'}
	{size}
	color={meta.fg}
	label={meta.label}
/>
