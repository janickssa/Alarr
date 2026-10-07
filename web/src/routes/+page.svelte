<script lang="ts">
	import { APP_NAME } from '#lib';
</script>

<svelte:head>
	<title>{APP_NAME}</title>
</svelte:head>

<main>
	<h1>{APP_NAME}</h1>
	<p>Homelab control center. The UI is built in phase 2.</p>
</main>

<style>
	:global(body) {
		margin: 0;
		background: #161826;
		color: #e9e9ed;
		font-family: system-ui, sans-serif;
	}
	main {
		padding: 24px;
	}
	h1 {
		font-weight: 500;
	}
</style>
