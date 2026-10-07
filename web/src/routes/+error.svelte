<script lang="ts">
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import EmptyState from '#lib/components/EmptyState.svelte';
</script>

<svelte:head><title>{page.status === 404 ? 'Not found' : 'Error'}</title></svelte:head>

<div class="page">
	<EmptyState
		icon={page.status === 404 ? 'ph-compass' : 'ph-warning'}
		title={page.status === 404 ? 'This page does not exist' : 'Something went wrong'}
		detail={page.error?.message}
	>
		<a class="btn btn-primary" href={resolve('/')}>Go to Home</a>
	</EmptyState>
</div>
