<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { onMount } from 'svelte';
	import { session } from '#lib/session.svelte.js';

	onMount(() => {
		void goto(resolve(session.isOwner ? '/settings/services' : '/settings/account'), {
			replace: true
		});
	});
</script>
