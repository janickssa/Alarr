<script lang="ts">
	import Icon from '../Icon.svelte';

	let {
		name,
		state,
		progress,
		size,
		speed,
		eta,
		source
	}: {
		name: string;
		state: string;
		progress: number;
		size: string;
		speed?: string;
		eta?: string;
		source?: string;
	} = $props();
</script>

<div class="q">
	<div class="top">
		<span class="mono name" title={name}>{name}</span>
		<span class="state"><Icon name="ph-circle-notch" />{state}</span>
	</div>
	<div
		class="track"
		role="progressbar"
		aria-label="Progress"
		aria-valuenow={Math.round(progress * 100)}
		aria-valuemin={0}
		aria-valuemax={100}
	>
		<div class="fill" style:width="{progress * 100}%"></div>
	</div>
	<div class="meta mono">
		<span>{Math.round(progress * 100)} %</span><span>{size}</span>
		{#if speed}<span>↓ {speed}</span>{/if}
		{#if eta}<span>ETA {eta}</span>{/if}
		{#if source}<span class="src">{source}</span>{/if}
	</div>
</div>

<style>
	.q {
		background: var(--color-surface);
		border-radius: var(--radius-md);
		padding: 12px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.top {
		display: flex;
		justify-content: space-between;
		gap: 10px;
	}
	.name {
		font-size: 12.5px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.state {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		color: var(--health-progress);
		font-size: 11.5px;
		flex: none;
	}
	.track {
		height: 4px;
		border-radius: 2px;
		background: var(--color-neutral-800);
	}
	.fill {
		height: 100%;
		border-radius: 2px;
		background: var(--health-progress);
	}
	.meta {
		display: flex;
		gap: 14px;
		font-size: 11.5px;
		color: var(--color-neutral-400);
	}
	.src {
		margin-left: auto;
	}
</style>
