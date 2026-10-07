<script lang="ts">
	import { toasts } from '#lib/toasts.svelte.js';
	import Icon from './Icon.svelte';

	const ICON = {
		progress: { name: 'ph-circle-notch', fill: false, color: 'var(--health-progress)' },
		success: { name: 'ph-check-circle', fill: true, color: 'var(--health-healthy)' },
		error: { name: 'ph-x-circle', fill: true, color: 'var(--health-down)' },
		info: { name: 'ph-info', fill: true, color: 'var(--color-accent)' }
	};
</script>

<div class="toaster" aria-live="polite">
	{#each toasts.items as t (t.id)}
		{@const ic = ICON[t.kind]}
		<div class="toast" role={t.kind === 'error' ? 'alert' : 'status'}>
			<Icon name={ic.name} fill={ic.fill} size={18} color={ic.color} spin={t.kind === 'progress'} />
			<div class="txt">
				<span class="title">{t.title}</span>
				{#if t.sub}<span class="sub" class:mono={t.mono}>{t.sub}</span>{/if}
			</div>
			{#if t.action}
				<button class="btn btn-ghost btn-sm" onclick={() => t.action?.run()}
					>{t.action.label}</button
				>
			{/if}
			<button
				class="btn btn-ghost btn-icon close"
				aria-label="Dismiss"
				onclick={() => toasts.dismiss(t.id)}
			>
				<Icon name="ph-x" size={14} />
			</button>
		</div>
	{/each}
</div>

<style>
	.toaster {
		position: fixed;
		right: 24px;
		bottom: 24px;
		z-index: 50;
		display: flex;
		flex-direction: column;
		gap: 8px;
		width: min(380px, calc(100vw - 32px));
	}
	/* Phones: bottom-centre, above the tab bar. */
	@media (max-width: 767px) {
		.toaster {
			right: 16px;
			left: 16px;
			width: auto;
			bottom: calc(var(--tabbar-h) + var(--safe-bottom) + 12px);
			align-items: stretch;
		}
		/* Keep the screen usable: only the two newest toasts. */
		.toast:not(:nth-last-child(-n + 2)) {
			display: none;
		}
	}
	.toast {
		background: var(--color-surface);
		border-radius: 10px;
		box-shadow: var(--shadow-md);
		padding: 10px 8px 10px 12px;
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.txt {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}
	.title {
		font-size: 13px;
	}
	.sub {
		font-size: 11.5px;
		color: var(--color-neutral-400);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.close {
		width: 24px;
		height: 24px;
		color: var(--color-neutral-400);
	}
</style>
