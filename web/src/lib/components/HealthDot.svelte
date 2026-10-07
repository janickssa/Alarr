<script lang="ts">
	// Status dot. Only used next to a text label (rows and tables).
	import type { Health } from '#lib/api/client.js';
	import { HEALTH } from '#lib/health.js';

	let { health, label }: { health: Health; label?: string } = $props();
	const meta = $derived(HEALTH[health] ?? HEALTH.unknown);
</script>

<span class="dot-label">
	<span class="dot" style:background={meta.fg} style:box-shadow="0 0 0 3px {meta.fill}"></span>
	{label ?? meta.label}
</span>

<style>
	.dot-label {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		white-space: nowrap;
	}
	.dot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		flex: none;
	}
</style>
