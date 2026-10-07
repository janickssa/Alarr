<script lang="ts">
	// Activity log: who did what, when, to what, and the result. Live via action events.
	import { onMount } from 'svelte';
	import { api, errorMessage, unwrap, type ActivityEntry } from '#lib/api/client.js';
	import EmptyState from '#lib/components/EmptyState.svelte';
	import ErrorState from '#lib/components/ErrorState.svelte';
	import HealthBadge from '#lib/components/HealthBadge.svelte';
	import RelativeTime from '#lib/components/RelativeTime.svelte';
	import SearchField from '#lib/components/SearchField.svelte';
	import { live } from '#lib/live.svelte.js';

	let entries = $state<ActivityEntry[]>([]);
	let loaded = $state(false);
	let more = $state(true);
	let error = $state('');
	let query = $state('');

	async function load(before?: number) {
		try {
			const page = unwrap(await api.GET('/activity', { params: { query: { limit: 50, before } } }));
			entries = before ? [...entries, ...page] : page;
			more = page.length === 50;
			loaded = true;
			error = '';
		} catch (e) {
			error = errorMessage(e);
		}
	}

	onMount(() => {
		void load();
		return live.onAction(({ entry }) => {
			const i = entries.findIndex((e) => e.id === entry.id);
			if (i >= 0) entries[i] = entry;
			else entries = [entry, ...entries];
		});
	});

	const shown = $derived(
		entries.filter(
			(e) =>
				!query ||
				`${e.action} ${e.target} ${e.username ?? ''} ${e.detail ?? ''}`
					.toLowerCase()
					.includes(query.toLowerCase())
		)
	);
	const label = (e: ActivityEntry) =>
		e.action
			.replace(/^guest\.power\./, 'Power: ')
			.replace(/^service\.action\./, 'Action: ')
			.replace(/^service\./, 'Service ')
			.replace(/^auth\./, 'Auth: ')
			.replace(/^user\./, 'User ')
			.replace(/^device\./, 'Device ')
			.replace(/^pairing\./, 'Pairing ')
			.replace(/^settings\./, 'Settings ');
</script>

<svelte:head><title>Activity</title></svelte:head>

<div class="page">
	<div class="page-header">
		<h1 class="page-title">Activity</h1>
		<span class="spacer"></span>
		<SearchField
			bind:value={query}
			placeholder="Filter by action, target or user"
			label="Filter activity"
		/>
	</div>
	{#if error && !loaded}
		<ErrorState title="Can't load the activity log" detail={error} onretry={() => load()} />
	{:else if !loaded}
		{#each Array(8), i (i)}<div class="skeleton" style="height:36px"></div>{/each}
	{:else if shown.length === 0}
		<EmptyState
			title="No activity yet"
			detail="Sign-ins, power actions and changes show up here."
		/>
	{:else}
		<div class="card tablecard">
			<table class="table">
				<thead
					><tr
						><th>When</th><th>Who</th><th>Action</th><th class="hide-sm">Target</th><th>Result</th
						><th class="hide-sm">Detail</th></tr
					></thead
				>
				<tbody>
					{#each shown as e (e.id)}
						<tr>
							<td class="caption nowrap"><RelativeTime value={e.at} /></td>
							<td>{e.username ?? '–'}</td>
							<td>{label(e)}</td>
							<td class="mono caption hide-sm">{e.target}</td>
							<td>
								<HealthBadge
									health={e.result === 'ok'
										? 'healthy'
										: e.result === 'pending'
											? 'in_progress'
											: 'down'}
									label={e.result === 'ok' ? 'OK' : e.result === 'pending' ? 'Running' : 'Failed'}
									small
								/>
							</td>
							<td class="caption muted detail hide-sm">{e.detail ?? ''}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
		{#if more && !query}
			<button class="btn btn-secondary more" onclick={() => load(entries[entries.length - 1]?.id)}
				>Load older</button
			>
		{/if}
	{/if}
</div>

<style>
	.tablecard {
		padding: 4px 8px;
		overflow-x: auto;
	}
	.nowrap {
		white-space: nowrap;
	}
	.detail {
		max-width: 360px;
	}
	.more {
		align-self: center;
	}
</style>
