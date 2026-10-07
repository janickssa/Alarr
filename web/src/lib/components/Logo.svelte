<script lang="ts">
	// The Quorum mark: three nodes holding one core (batch 1, 1a).
	import { APP_NAME } from '#lib';
	let { size = 28, wordmark = true }: { size?: number; wordmark?: boolean } = $props();
</script>

<span class="lockup">
	<svg width={size} height={size} viewBox="0 0 48 48" fill="none" aria-hidden="true">
		<path
			d="M24 10 L11 34 L37 34 Z"
			stroke="#b5abfc"
			stroke-width="2.4"
			stroke-linejoin="round"
			opacity=".55"
		/>
		<circle cx="24" cy="10" r="5" fill="#161826" stroke="#d2cefd" stroke-width="2.6" />
		<circle cx="11" cy="34" r="5" fill="#161826" stroke="#d2cefd" stroke-width="2.6" />
		<circle cx="37" cy="34" r="5" fill="#161826" stroke="#d2cefd" stroke-width="2.6" />
		<circle cx="24" cy="26" r="3.4" fill="#9184d9" />
	</svg>
	{#if wordmark}<span class="name">{APP_NAME}</span>{/if}
</span>

<style>
	.lockup {
		display: inline-flex;
		align-items: center;
		gap: 10px;
	}
	.name {
		font-size: 18px;
		font-weight: 500;
		letter-spacing: -0.015em;
	}
</style>
