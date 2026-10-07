<script lang="ts">
	// Power confirmations (batch 1, 1h): normal for shutdown/reboot, destructive for hard stop.
	import { guestId } from '#lib/format.js';
	import { power, verb } from '#lib/power.svelte.js';
	import Dialog from './Dialog.svelte';
	import Icon from './Icon.svelte';

	const req = $derived(power.pending);
	const destructive = $derived(req?.action === 'stop');
	let open = $derived(req != null);

	const text = $derived.by(() => {
		if (!req) return '';
		const linked = req.linked.length ? ` Linked: ${req.linked.join(', ')}.` : '';
		switch (req.action) {
			case 'reboot':
				return `${req.guest.name} will be offline for a short while.${linked}`;
			case 'shutdown':
				return `${req.guest.name} shuts down cleanly and stays off until you start it.${linked}`;
			case 'stop':
				return `This is like pulling the power cord. Unsaved data in ${req.guest.name} may be lost.${linked} Use Shutdown for a clean stop.`;
			default:
				return '';
		}
	});
	const titleVerb = $derived(
		req
			? { reboot: 'Reboot', shutdown: 'Shut down', stop: 'Hard stop', start: 'Start' }[req.action]
			: ''
	);
</script>

{#if req}
	<Dialog
		bind:open
		title="{titleVerb} {req.guest.name}?"
		{destructive}
		onclose={() => power.answer(null)}
	>
		<div class="title">
			{#if destructive}<Icon name="ph-warning-octagon" fill color="var(--health-down)" />{/if}
			{titleVerb}
			{req.guest.name}?
		</div>
		<div class="target">
			<Icon name="ph-cube" color="var(--color-neutral-400)" />
			<span class="mono"
				>{guestId(req.guest.guest_type, req.guest.vmid)} · {req.guest.name} · {req.guest.node}</span
			>
		</div>
		<p class="text">{text}</p>
		{#snippet actions()}
			{#if destructive}
				<button class="btn btn-secondary" onclick={() => power.answer('shutdown')}
					>Shutdown instead</button
				>
				<button class="btn btn-danger" onclick={() => power.answer('stop')}>
					<Icon name="ph-stop" />Stop now
				</button>
			{:else if req}
				<button class="btn btn-secondary" onclick={() => power.answer(null)}>Cancel</button>
				<button class="btn btn-primary" onclick={() => req && power.answer(req.action)}>
					<Icon name={req.action === 'reboot' ? 'ph-arrow-clockwise' : 'ph-power'} />{verb(
						req.action
					)[0]}
				</button>
			{/if}
		{/snippet}
	</Dialog>
{/if}

<style>
	.title {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 18px;
		font-weight: 500;
	}
	.target {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 10px;
		border-radius: var(--radius-md);
		background: var(--color-bg);
		font-size: 12.5px;
	}
	.text {
		font-size: 13px;
		color: var(--color-neutral-300);
		text-wrap: pretty;
	}
</style>
