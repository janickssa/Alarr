<script lang="ts">
	// Right-side detail panel. Esc or the close button closes it. On phones it becomes a
	// pushed full-screen page with a back button, closed by an edge swipe too.
	import type { Snippet } from 'svelte';
	import Icon from './Icon.svelte';

	let {
		title,
		onclose,
		children,
		header,
		showTitle = true
	}: {
		title: string;
		onclose: () => void;
		children: Snippet;
		header?: Snippet;
		showTitle?: boolean;
	} = $props();
	let panel: HTMLElement | undefined = $state();

	$effect(() => {
		panel?.focus();
	});

	let swipe: { x: number; y: number } | null = null;
	function ontouchstart(e: TouchEvent) {
		const t = e.touches[0];
		swipe = t.clientX < 24 ? { x: t.clientX, y: t.clientY } : null;
	}
	function ontouchend(e: TouchEvent) {
		if (!swipe) return;
		const t = e.changedTouches[0];
		if (t.clientX - swipe.x > 80 && Math.abs(t.clientY - swipe.y) < 50) onclose();
		swipe = null;
	}
</script>

<svelte:window
	onkeydown={(e) => {
		if (e.key === 'Escape' && !document.querySelector('dialog[open]')) onclose();
	}}
/>

<aside class="panel" aria-label={title} tabindex="-1" bind:this={panel} {ontouchstart} {ontouchend}>
	<div class="bar">
		<button class="back" aria-label="Back" onclick={onclose}>
			<Icon name="ph-caret-left" size={22} />
		</button>
		<div class="hdr">
			{#if header}{@render header()}{:else}<span class="t" class:visually-hidden={!showTitle}
					>{title}</span
				>{/if}
		</div>
		<button class="btn btn-ghost btn-icon close" aria-label="Close panel" onclick={onclose}>
			<Icon name="ph-x" />
		</button>
	</div>
	<div class="content">{@render children()}</div>
</aside>

<style>
	.panel {
		position: sticky;
		top: var(--header-h);
		height: calc(100vh - var(--header-h));
		width: min(520px, 45vw);
		flex: none;
		background: var(--color-surface);
		box-shadow: var(--shadow-md);
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}
	.panel:focus {
		outline: none;
	}
	.bar {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 14px 14px 10px 18px;
	}
	.hdr {
		flex: 1;
		min-width: 0;
	}
	.t {
		font: var(--text-title);
	}
	.back {
		display: none;
	}
	@media (max-width: 767px) {
		.panel {
			position: fixed;
			inset: 0 0 calc(var(--tabbar-h) + var(--safe-bottom)) 0;
			z-index: 25;
			width: auto;
			height: auto;
			padding-top: var(--safe-top);
			box-shadow: none;
			background: var(--color-bg);
		}
		.close {
			display: none;
		}
		.back {
			display: grid;
			place-items: center;
			width: 44px;
			height: 44px;
			margin-left: -10px;
			border: 0;
			background: none;
			color: var(--color-accent-300);
			cursor: pointer;
		}
		.bar {
			padding: 6px 16px 6px 16px;
		}
		.content {
			padding: 0 16px 24px;
		}
	}
	.content {
		flex: 1;
		overflow: auto;
		padding: 0 18px 24px;
		display: flex;
		flex-direction: column;
		gap: 16px;
	}
</style>
