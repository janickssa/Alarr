<script lang="ts">
	let {
		checked = $bindable(false),
		label,
		disabled = false,
		onchange
	}: {
		checked?: boolean;
		label: string;
		disabled?: boolean;
		onchange?: (v: boolean) => void;
	} = $props();
</script>

<button
	type="button"
	role="switch"
	class="switch"
	aria-checked={checked}
	aria-label={label}
	{disabled}
	onclick={() => {
		checked = !checked;
		onchange?.(checked);
	}}
>
	<span class="knob"></span>
</button>

<style>
	.switch {
		width: 34px;
		height: 20px;
		border-radius: 10px;
		border: 1px solid var(--color-divider);
		background: var(--color-neutral-900);
		padding: 0;
		position: relative;
		cursor: pointer;
		flex: none;
	}
	.switch[aria-checked='true'] {
		border-color: var(--color-accent);
		background: color-mix(in srgb, var(--color-accent) 22%, transparent);
	}
	.knob {
		position: absolute;
		top: 2px;
		left: 2px;
		width: 14px;
		height: 14px;
		border-radius: 50%;
		background: var(--color-neutral-400);
		transition: transform 0.15s;
	}
	.switch[aria-checked='true'] .knob {
		transform: translateX(14px);
		background: var(--color-accent);
	}
	.switch:disabled {
		opacity: 0.45;
		cursor: not-allowed;
	}
</style>
