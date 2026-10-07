<script lang="ts">
	let {
		title,
		meta,
		image,
		isNew = false,
		colors = ['#3b3560', '#1b1d2b']
	}: {
		title: string;
		meta?: string;
		image?: string;
		isNew?: boolean;
		colors?: [string, string];
	} = $props();
</script>

<div class="poster-card">
	<div
		class="poster"
		style:background={image
			? `center / cover no-repeat url("${image}"), linear-gradient(170deg, ${colors[0]}, ${colors[1]})`
			: `linear-gradient(170deg, ${colors[0]}, ${colors[1]})`}
	>
		{#if isNew}<span class="tag tag-accent new">New</span>{/if}
	</div>
	<span class="t">{title}</span>
	{#if meta}<span class="m">{meta}</span>{/if}
</div>

<style>
	.poster-card {
		width: 96px;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.poster {
		aspect-ratio: 2 / 3;
		border-radius: var(--radius-md);
		position: relative;
		box-shadow: 0 0 0 1px var(--color-border);
	}
	.new {
		position: absolute;
		top: 6px;
		left: 6px;
		font-size: 10px;
		padding: 1px 6px;
	}
	.t {
		font-size: 12.5px;
		line-height: 1.25;
	}
	.m {
		font-size: 11px;
		color: var(--color-neutral-500);
	}
</style>
