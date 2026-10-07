<script lang="ts" generics="T extends string">
	// Segmented control (filters with counts, range switches).
	let {
		options,
		value = $bindable(),
		label,
		small = false
	}: {
		options: { value: T; label: string; count?: number }[];
		value: T;
		label: string;
		small?: boolean;
	} = $props();
</script>

<div class="seg" class:seg-sm={small} role="radiogroup" aria-label={label}>
	{#each options as o (o.value)}
		<button
			type="button"
			class="seg-opt"
			role="radio"
			aria-checked={value === o.value}
			onclick={() => (value = o.value)}
		>
			{o.label}
			{#if o.count != null}<span class="faint num">{o.count}</span>{/if}
		</button>
	{/each}
</div>
