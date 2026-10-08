<script lang="ts">
	// Pull to refresh on phones: pull down from the top of the page and release to reload
	// every page's data (pages using refetchOnLive), the service list and alerts.
	import { alerts } from '#lib/alerts.svelte.js';
	import { live } from '#lib/live.svelte.js';
	import { session } from '#lib/session.svelte.js';
	import Icon from '../Icon.svelte';

	const TRIGGER = 64;
	let start: number | null = null;
	let pull = $state(0);
	let busy = $state(false);

	function ontouchstart(e: TouchEvent) {
		const blocked =
			window.innerWidth >= 768 ||
			window.scrollY > 0 ||
			busy ||
			document.querySelector('dialog[open], [role="menu"]') ||
			(e.target as HTMLElement).closest('[role="listbox"], .panel');
		start = blocked ? null : e.touches[0].clientY;
	}
	function ontouchmove(e: TouchEvent) {
		if (start == null) return;
		const dy = e.touches[0].clientY - start;
		pull = dy > 0 && window.scrollY <= 0 ? Math.min(dy * 0.5, 90) : 0;
	}
	async function ontouchend() {
		if (start == null) return;
		start = null;
		if (pull < TRIGGER) {
			pull = 0;
			return;
		}
		busy = true;
		pull = 48;
		live.refresh();
		await Promise.all([session.loadServices().catch(() => {}), alerts.refresh().catch(() => {})]);
		await new Promise((r) => setTimeout(r, 400));
		busy = false;
		pull = 0;
	}
</script>

<svelte:window {ontouchstart} {ontouchmove} {ontouchend} ontouchcancel={() => (pull = 0)} />

{#if pull > 0}
	<div class="ptr" style:height="{pull}px" aria-live="polite">
		<span class="ico" class:ready={pull >= TRIGGER || busy} style:rotate="{busy ? 0 : pull * 3}deg">
			<Icon name={busy ? 'ph-circle-notch' : 'ph-arrow-down'} spin={busy} size={20} />
		</span>
		{#if busy}<span class="visually-hidden">Refreshing</span>{/if}
	</div>
{/if}

<style>
	.ptr {
		display: grid;
		place-items: center;
		overflow: hidden;
		color: var(--color-neutral-400);
	}
	.ico {
		display: grid;
		place-items: center;
		width: 36px;
		height: 36px;
		border-radius: 50%;
		background: var(--color-surface);
		box-shadow: var(--shadow-sm);
	}
	.ready {
		color: var(--color-accent-300);
	}
</style>
