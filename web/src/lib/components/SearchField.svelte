<script lang="ts">
	// Search input with the "/" shortcut hint.
	import Icon from './Icon.svelte';

	let {
		value = $bindable(''),
		placeholder = 'Search…',
		label = 'Search',
		shortcut = false,
		input = $bindable<HTMLInputElement | undefined>(),
		width = '300px',
		onkeydown,
		onfocus
	}: {
		value?: string;
		placeholder?: string;
		label?: string;
		shortcut?: boolean;
		input?: HTMLInputElement;
		width?: string;
		onkeydown?: (e: KeyboardEvent) => void;
		onfocus?: () => void;
	} = $props();
</script>

<div class="search" style:width>
	<span class="glass"><Icon name="ph-magnifying-glass" /></span>
	<input
		bind:this={input}
		bind:value
		class="input"
		type="search"
		{placeholder}
		aria-label={label}
		{onkeydown}
		{onfocus}
	/>
	{#if shortcut && !value}<kbd class="mono">/</kbd>{/if}
</div>

<style>
	.search {
		position: relative;
		max-width: 100%;
	}
	.glass {
		position: absolute;
		left: 10px;
		top: 50%;
		transform: translateY(-50%);
		color: var(--color-neutral-400);
		display: flex;
	}
	.input {
		padding-left: 32px;
	}
	input::-webkit-search-cancel-button {
		filter: grayscale(1);
	}
	kbd {
		position: absolute;
		right: 8px;
		top: 50%;
		transform: translateY(-50%);
		font-size: 11px;
		padding: 1px 6px;
		border-radius: 4px;
		box-shadow: 0 0 0 1px var(--color-divider);
		color: var(--color-neutral-400);
		pointer-events: none;
	}
</style>
