<script lang="ts">
	// Loading placeholder shaped like a standard service card (1.2 s shimmer).
	let { lines = 1 }: { lines?: number } = $props();
</script>

<div class="card" aria-busy="true" aria-label="Loading">
	<div class="row">
		<div class="skeleton" style="width:30px;height:30px;border-radius:8px"></div>
		<div class="skeleton" style="height:10px;width:80px;border-radius:4px"></div>
	</div>
	<div class="skeleton" style="height:20px;width:110px"></div>
	{#each Array(lines), i (i)}
		<div class="skeleton dim" style="height:9px;width:140px;border-radius:4px"></div>
	{/each}
</div>

<style>
	.row {
		display: flex;
		align-items: center;
		gap: 10px;
	}
</style>
