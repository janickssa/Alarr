<script lang="ts">
	// Power menu for a guest: Start, Shutdown, Reboot, Stop (hard). Keyboard: arrows, Esc.
	import type { Guest, PowerAction } from '#lib/api/client.js';
	import { guestId } from '#lib/format.js';
	import { power } from '#lib/power.svelte.js';
	import Icon from './Icon.svelte';

	let {
		guest,
		linked = [],
		disabled = false,
		label = 'Power'
	}: { guest: Guest; linked?: string[]; disabled?: boolean; label?: string } = $props();

	let open = $state(false);
	let menu: HTMLDivElement | undefined = $state();
	let button: HTMLButtonElement | undefined = $state();
	const running = $derived(guest.status === 'running');

	const items = $derived<{ action: PowerAction; label: string; icon: string; enabled: boolean }[]>([
		{ action: 'start', label: 'Start', icon: 'ph-play', enabled: !running },
		{ action: 'shutdown', label: 'Shutdown', icon: 'ph-power', enabled: running },
		{ action: 'reboot', label: 'Reboot', icon: 'ph-arrow-clockwise', enabled: running },
		{ action: 'stop', label: 'Stop (hard)', icon: 'ph-stop', enabled: running }
	]);

	function focusItem(delta: number) {
		const els = [...(menu?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? [])];
		if (!els.length) return;
		const i = els.indexOf(document.activeElement as HTMLButtonElement);
		els[(i + delta + els.length) % els.length].focus();
	}

	function toggle() {
		open = !open;
		if (open) queueMicrotask(() => focusItem(1));
	}

	function choose(action: PowerAction) {
		open = false;
		button?.focus();
		void power.run(guest, action, linked);
	}

	function onkeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.stopPropagation();
			open = false;
			button?.focus();
		} else if (e.key === 'ArrowDown') {
			e.preventDefault();
			focusItem(1);
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			focusItem(-1);
		}
	}
</script>

<svelte:window
	onclick={(e) => {
		if (open && !menu?.contains(e.target as Node) && !button?.contains(e.target as Node))
			open = false;
	}}
/>

<div class="power">
	<button
		bind:this={button}
		class="btn btn-secondary btn-icon"
		aria-haspopup="menu"
		aria-expanded={open}
		aria-label="{label}: {guest.name}"
		title={disabled ? 'Not allowed for your role' : label}
		disabled={disabled || guest.template}
		onclick={(e) => {
			e.stopPropagation();
			toggle();
		}}
	>
		<Icon name="ph-power" />
	</button>
	{#if open}
		<div class="menu" role="menu" tabindex="-1" bind:this={menu} {onkeydown}>
			<div class="head mono">
				{guestId(guest.guest_type, guest.vmid)} · {guest.name} · {guest.status}
			</div>
			{#each items as item (item.action)}
				{#if item.action === 'stop'}<div class="sep" role="separator"></div>{/if}
				<button
					role="menuitem"
					class="item"
					class:danger={item.action === 'stop'}
					disabled={!item.enabled}
					onclick={() => choose(item.action)}
				>
					<Icon name={item.icon} />{item.label}
				</button>
			{/each}
		</div>
	{/if}
</div>

<style>
	.power {
		position: relative;
		display: inline-block;
	}
	.menu {
		position: absolute;
		right: 0;
		top: calc(100% + 4px);
		z-index: 30;
		min-width: 210px;
		background: var(--color-surface);
		border-radius: var(--radius-md);
		box-shadow: var(--shadow-md);
		padding: 4px;
		display: flex;
		flex-direction: column;
	}
	.head {
		padding: 6px 10px;
		font-size: 11px;
		color: var(--color-neutral-500);
		white-space: nowrap;
	}
	.item {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 10px;
		border-radius: 6px;
		font-size: 13.5px;
		background: none;
		border: 0;
		text-align: left;
		cursor: pointer;
	}
	.item:hover:not(:disabled),
	.item:focus-visible {
		background: color-mix(in srgb, var(--color-text) 7%, transparent);
		outline: none;
	}
	.item:disabled {
		opacity: 0.45;
		cursor: default;
	}
	.danger {
		color: var(--health-down);
	}
	.sep {
		height: 1px;
		margin: 4px 8px;
		background: var(--color-divider);
	}
</style>
