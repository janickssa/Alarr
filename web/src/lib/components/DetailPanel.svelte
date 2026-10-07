<script lang="ts">
	// Right-side detail panel. Esc or the close button closes it.
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
</script>

<svelte:window
	onkeydown={(e) => {
		if (e.key === 'Escape' && !document.querySelector('dialog[open]')) onclose();
	}}
/>

<aside class="panel" aria-label={title} tabindex="-1" bind:this={panel}>
	<div class="bar">
		<div class="hdr">
			{#if header}{@render header()}{:else}<span class="t" class:visually-hidden={!showTitle}
					>{title}</span
				>{/if}
		</div>
		<button class="btn btn-ghost btn-icon" aria-label="Close panel" onclick={onclose}>
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
	.content {
		flex: 1;
		overflow: auto;
		padding: 0 18px 24px;
		display: flex;
		flex-direction: column;
		gap: 16px;
	}
</style>
