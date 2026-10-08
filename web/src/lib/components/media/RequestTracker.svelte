<script lang="ts">
	// Request tracker stepper: Requested → Approved → Grabbed → Downloading → Imported →
	// Available. The current step is blue, or amber with a reason when stuck.
	import Icon from '../Icon.svelte';

	const STEPS = ['Requested', 'Approved', 'Grabbed', 'Downloading', 'Imported', 'Available'];

	type StepState = 'done' | 'current' | 'stuck' | 'todo';
	let {
		title,
		meta,
		done = 0,
		stuck = false,
		subs = [],
		steps
	}: {
		title: string;
		meta?: string;
		done?: number;
		stuck?: boolean;
		subs?: string[];
		/** Explicit step states (from the API); overrides done/stuck/subs. */
		steps?: { label: string; state: StepState; detail?: string | null }[];
	} = $props();

	const items = $derived(
		steps ??
			STEPS.map((label, i) => ({
				label,
				state: (i < done
					? 'done'
					: i === done
						? stuck
							? 'stuck'
							: 'current'
						: 'todo') as StepState,
				detail: i <= done ? (subs[i] ?? '') : ''
			}))
	);
</script>

<div class="tracker">
	<div class="top">
		<span class="title">{title}</span>
		{#if meta}<span class="meta">{meta}</span>{/if}
	</div>
	<ol class="steps">
		{#each items as step (step.label)}
			{@const state = step.state}
			<li
				class="step {state}"
				aria-current={state === 'current' || state === 'stuck' ? 'step' : undefined}
			>
				<div class="bar"></div>
				<div class="lbl">
					<Icon
						name={state === 'done'
							? 'ph-check-circle'
							: state === 'stuck'
								? 'ph-warning'
								: state === 'current'
									? 'ph-circle-notch'
									: 'ph-circle'}
						fill={state === 'done' || state === 'stuck'}
						size={13}
					/><span class="t">{step.label}</span>
				</div>
				<span class="sub">{step.detail ?? ''}</span>
			</li>
		{/each}
	</ol>
	{#each items.filter((i) => i.state === 'current' || i.state === 'stuck') as now (now.label)}
		<span class="now {now.state}">{now.label}{now.detail ? ` · ${now.detail}` : ''}</span>
	{/each}
</div>

<style>
	.now {
		display: none;
	}
	/* Phones: the bars only, plus one line for the step it is waiting on. */
	@media (max-width: 767px) {
		.t,
		.sub {
			display: none;
		}
		.lbl {
			justify-content: center;
		}
		.now {
			display: block;
			font-size: 13px;
			color: var(--health-progress);
		}
		.now.stuck {
			color: var(--health-warning);
		}
		.top {
			flex-direction: column;
			gap: 2px;
		}
	}
	.tracker {
		background: var(--color-surface);
		border-radius: var(--radius-md);
		padding: 14px 16px;
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	.top {
		display: flex;
		justify-content: space-between;
		align-items: baseline;
		gap: 8px;
	}
	.title {
		font-size: 14px;
	}
	.meta {
		font-size: 11.5px;
		color: var(--color-neutral-400);
	}
	.steps {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		grid-template-columns: repeat(6, minmax(0, 1fr));
		gap: 4px;
	}
	.step {
		display: flex;
		flex-direction: column;
		gap: 7px;
		min-width: 0;
	}
	.bar {
		height: 3px;
		border-radius: 2px;
		background: var(--color-border);
	}
	.lbl {
		display: flex;
		align-items: center;
		gap: 5px;
		font-size: 12px;
		color: var(--color-neutral-500);
	}
	.sub {
		font-size: 11px;
		color: var(--color-neutral-500);
	}
	.done .bar {
		background: var(--color-neutral-300);
	}
	.done .lbl {
		color: var(--color-text);
	}
	.current .bar,
	.current .lbl,
	.current .sub {
		color: var(--health-progress);
		background: none;
	}
	.current .bar {
		background: var(--health-progress);
	}
	.stuck .bar {
		background: var(--health-warning);
	}
	.stuck .lbl,
	.stuck .sub {
		color: var(--health-warning);
	}
</style>
