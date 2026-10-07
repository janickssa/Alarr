<script lang="ts">
	import { page } from '$app/state';
	import {
		api,
		errorMessage,
		unwrap,
		type CatalogueEntry,
		type ServiceView
	} from '#lib/api/client.js';
	import ErrorState from '#lib/components/ErrorState.svelte';
	import ServiceForm from '#lib/components/settings/ServiceForm.svelte';

	let service = $state<ServiceView | null>(null);
	let entry = $state<CatalogueEntry | null>(null);
	let error = $state('');

	async function load(id: string) {
		try {
			const [s, cat] = await Promise.all([
				api.GET('/services/{id}', { params: { path: { id } } }),
				api.GET('/services/catalogue')
			]);
			service = unwrap(s);
			const all = unwrap(cat);
			entry =
				all.find((c) => c.preset === service?.preset) ??
				all.find((c) => c.preset === (service?.kind === 'proxmox' ? 'proxmox' : 'custom')) ??
				null;
		} catch (e) {
			error = errorMessage(e);
		}
	}
	$effect(() => {
		void load(page.params.id ?? '');
	});
</script>

<svelte:head><title>Configure {service?.name ?? 'service'}</title></svelte:head>

{#if error}
	<ErrorState title="Can't load this service" detail={error} />
{:else if service && entry}
	{#key service.id}<ServiceForm {entry} existing={service} />{/key}
{/if}
