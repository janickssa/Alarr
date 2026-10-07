<script lang="ts">
	let {
		title,
		detail,
		progress,
		position,
		transcode = false,
		reason,
		tags = []
	}: {
		title: string;
		detail: string;
		progress: number;
		position: string;
		transcode?: boolean;
		reason?: string;
		/** Extra state tags, e.g. Remote or Paused. */
		tags?: string[];
	} = $props();
</script>

<div class="np">
	<div class="poster"></div>
	<div class="body">
		<div class="top">
			<span class="title">{title}</span>
			{#each tags as t (t)}<span class="tag tag-outline">{t}</span>{/each}
			<span class="tag tag-neutral">{transcode ? 'Transcode' : 'Direct play'}</span>
		</div>
		<span class="detail">{detail}</span>
		<div class="prog">
			<div
				class="track"
				role="progressbar"
				aria-valuenow={Math.round(progress * 100)}
				aria-valuemin={0}
				aria-valuemax={100}
				aria-label="Playback position"
			>
				<div class="fill" style:width="{progress * 100}%"></div>
			</div>
			<span class="mono pos">{position}</span>
		</div>
		{#if reason}<span class="reason">Reason: {reason}</span>{/if}
	</div>
</div>

<style>
	.np {
		background: var(--color-surface);
		border-radius: var(--radius-md);
		padding: 12px;
		display: flex;
		gap: 12px;
	}
	.poster {
		width: 52px;
		aspect-ratio: 2 / 3;
		flex: none;
		border-radius: 6px;
		background: linear-gradient(170deg, #3b3560, #1d1f2e);
		box-shadow: 0 0 0 1px var(--color-border);
	}
	.body {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.top {
		display: flex;
		justify-content: space-between;
		gap: 8px;
	}
	.title {
		font-size: 14px;
		flex: 1;
		min-width: 0;
	}
	.detail {
		font-size: 12px;
		color: var(--color-neutral-400);
	}
	.prog {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.track {
		flex: 1;
		height: 4px;
		border-radius: 2px;
		background: var(--color-neutral-800);
	}
	.fill {
		height: 100%;
		border-radius: 2px;
		background: var(--color-neutral-300);
	}
	.pos {
		font-size: 11px;
		color: var(--color-neutral-400);
	}
	.reason {
		font-size: 11px;
		color: var(--color-neutral-500);
	}
</style>
