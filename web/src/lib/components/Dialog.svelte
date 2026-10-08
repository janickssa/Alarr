<script lang="ts">
	// Modal dialog on the native <dialog> element (focus trap and Esc built in).
	import type { Snippet } from 'svelte';

	let {
		open = $bindable(false),
		title,
		destructive = false,
		width = 440,
		onclose,
		children,
		actions
	}: {
		open?: boolean;
		title: string;
		destructive?: boolean;
		width?: number;
		onclose?: () => void;
		children: Snippet;
		actions?: Snippet;
	} = $props();

	let el: HTMLDialogElement | undefined = $state();

	$effect(() => {
		if (!el) return;
		if (open && !el.open) el.showModal();
		if (!open && el.open) el.close();
	});
</script>

<dialog
	bind:this={el}
	class:destructive
	style:width="min({width}px, calc(100vw - 32px))"
	aria-label={title}
	onclose={() => {
		open = false;
		onclose?.();
	}}
	onclick={(e) => {
		if (e.target === el) el?.close();
	}}
>
	<div class="body">
		{@render children()}
		{#if actions}<div class="actions">{@render actions()}</div>{/if}
	</div>
</dialog>

<style>
	dialog {
		border: 0;
		padding: 0;
		background: var(--color-surface);
		color: var(--color-text);
		border-radius: var(--radius-lg);
		box-shadow: var(--shadow-lg);
	}
	dialog.destructive {
		box-shadow:
			0 0 0 1px var(--health-down-edge),
			0 16px 40px rgba(0, 0, 0, 0.65);
	}
	dialog::backdrop {
		background: color-mix(in srgb, #0e0f18 60%, transparent);
	}
	.body {
		padding: 18px;
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	.actions {
		display: flex;
		gap: 8px;
		justify-content: flex-end;
		flex-wrap: wrap;
	}
	/* Phones: a bottom sheet with full-width, thumb-sized buttons. */
	@media (max-width: 767px) {
		dialog {
			width: 100% !important;
			max-width: 100%;
			margin: auto 0 0;
			border-radius: var(--radius-lg) var(--radius-lg) 0 0;
			animation: sheet-up 180ms ease-out;
		}
		.body {
			padding: 10px 16px calc(16px + var(--safe-bottom));
		}
		.body::before {
			content: '';
			align-self: center;
			width: 36px;
			height: 4px;
			border-radius: 2px;
			background: var(--color-border);
		}
		.actions {
			flex-direction: column-reverse;
		}
		.actions :global(.btn) {
			width: 100%;
			min-height: 48px;
			justify-content: center;
		}
	}
	@keyframes sheet-up {
		from {
			transform: translateY(40%);
			opacity: 0.4;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		dialog {
			animation: none;
		}
	}
</style>
