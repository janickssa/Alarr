<script lang="ts">
	// Tinted icon tile in the service accent. Also the logo slot.
	import Icon from './Icon.svelte';

	let {
		icon,
		hue,
		chroma = 0.045,
		size = 30
	}: { icon: string; hue: number; chroma?: number; size?: number } = $props();
	const neutral = $derived(chroma < 0.03);
</script>

<span
	class="tile"
	style:width="{size}px"
	style:height="{size}px"
	style:background={neutral ? `oklch(0.32 0.015 ${hue})` : `oklch(0.33 ${chroma} ${hue})`}
	aria-hidden="true"
>
	<Icon
		name={icon}
		size={Math.round(size * 0.57)}
		color={neutral ? `oklch(0.84 0.02 ${hue})` : `oklch(0.84 0.07 ${hue})`}
	/>
</span>

<style>
	.tile {
		flex: none;
		border-radius: var(--radius-md);
		display: inline-grid;
		place-items: center;
	}
</style>
