<script lang="ts">
	// Alerts: recent and active alerts (live), per-type settings for the owner (on/off,
	// channels, quiet hours), channel status with a test send, and this user's mutes.
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import {
		api,
		errorMessage,
		unwrap,
		type Alert,
		type AlertSettingsView,
		type AlertType,
		type TypeSettings
	} from '#lib/api/client.js';
	import { alerts } from '#lib/alerts.svelte.js';
	import EmptyState from '#lib/components/EmptyState.svelte';
	import ErrorState from '#lib/components/ErrorState.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import RelativeTime from '#lib/components/RelativeTime.svelte';
	import SegFilter from '#lib/components/SegFilter.svelte';
	import Switch from '#lib/components/Switch.svelte';
	import { duration } from '#lib/format.js';
	import { live } from '#lib/live.svelte.js';
	import { session } from '#lib/session.svelte.js';
	import { toasts } from '#lib/toasts.svelte.js';

	type Tab = 'alerts' | 'settings';
	let tab = $state<Tab>('alerts');
	let filter = $state<'active' | 'all'>('active');
	let list = $state<Alert[]>([]);
	let loaded = $state(false);
	let error = $state('');
	let view = $state<AlertSettingsView | null>(null);
	let draft = $state<TypeSettings[]>([]);
	let saving = $state(false);
	let mutes = $state<AlertType[]>([]);
	const owner = $derived(session.me?.user.role === 'owner');

	async function load() {
		try {
			list = unwrap(await api.GET('/alerts', { params: { query: { limit: 100 } } }));
			loaded = true;
			error = '';
		} catch (e) {
			error = errorMessage(e);
		}
	}

	async function loadSettings() {
		const [s, m] = await Promise.all([api.GET('/alerts/settings'), api.GET('/alerts/mutes')]);
		if (s.data) {
			view = s.data;
			draft = s.data.types.map((t) => ({
				alert_type: t.alert_type,
				enabled: t.enabled,
				channels: [...t.channels],
				quiet_hours: t.quiet_hours ? { ...t.quiet_hours } : undefined
			}));
		}
		if (m.data) mutes = m.data.types;
	}

	onMount(() => {
		void load();
		void loadSettings();
		return live.onAlert(({ alert }) => {
			const i = list.findIndex((a) => a.id === alert.id);
			if (i >= 0) list[i] = alert;
			else list = [alert, ...list];
		});
	});

	const shown = $derived(filter === 'active' ? list.filter((a) => !a.resolved_at) : list);
	const activeCount = $derived(list.filter((a) => !a.resolved_at).length);

	const SEVERITY: Record<string, { icon: string; color: string }> = {
		critical: { icon: 'ph-warning-octagon', color: 'var(--health-down)' },
		warning: { icon: 'ph-warning', color: 'var(--health-warning)' },
		info: { icon: 'ph-bell', color: 'var(--health-progress)' }
	};
	const RESULT: Record<string, string> = {
		sent: 'sent',
		failed: 'failed',
		quiet_hours: 'held (quiet hours)',
		not_configured: 'not set up',
		rate_limited: 'skipped (too many)'
	};
	const CHANNEL_LABEL: Record<string, string> = { ntfy: 'ntfy', push: 'iOS push' };

	function href(a: Alert): string {
		if (a.alert_type === 'node_offline' || a.alert_type === 'disk_full') return resolve('/proxmox');
		return a.service_id ? resolve('/services/[id]', { id: a.service_id }) : resolve('/alerts');
	}

	const dirty = $derived(
		view != null &&
			JSON.stringify(draft) !==
				JSON.stringify(
					view.types.map((t) => ({
						alert_type: t.alert_type,
						enabled: t.enabled,
						channels: t.channels,
						quiet_hours: t.quiet_hours ?? undefined
					}))
				)
	);

	function toggleChannel(t: TypeSettings, id: string) {
		t.channels = t.channels.includes(id)
			? t.channels.filter((c) => c !== id)
			: [...t.channels, id].sort();
	}

	async function save() {
		saving = true;
		try {
			view = unwrap(await api.PUT('/alerts/settings', { body: { types: draft } }));
			toasts.push({ kind: 'success', title: 'Alert settings saved' });
			await loadSettings();
		} catch (e) {
			toasts.push({ kind: 'error', title: "Couldn't save alert settings", sub: errorMessage(e) });
		} finally {
			saving = false;
		}
	}

	async function test(channel: string) {
		const t = toasts.push({
			kind: 'progress',
			title: `Sending a test to ${CHANNEL_LABEL[channel]}`
		});
		try {
			const r = unwrap(await api.POST('/alerts/test', { body: { channel } }));
			toasts.update(t, {
				kind: r.ok ? 'success' : 'error',
				title: r.ok ? `Test sent to ${CHANNEL_LABEL[channel]}` : 'Test failed',
				sub: r.message
			});
		} catch (e) {
			toasts.update(t, { kind: 'error', title: 'Test failed', sub: errorMessage(e) });
		}
	}

	async function toggleMute(type: AlertType) {
		const next = mutes.includes(type) ? mutes.filter((m) => m !== type) : [...mutes, type];
		try {
			mutes = unwrap(await api.PUT('/alerts/mutes', { body: { types: next } })).types;
			void alerts.refresh();
		} catch (e) {
			toasts.push({ kind: 'error', title: "Couldn't change mutes", sub: errorMessage(e) });
		}
	}

	function setQuiet(t: TypeSettings, on: boolean) {
		t.quiet_hours = on ? { start: '22:00', end: '07:00' } : undefined;
	}
</script>

<svelte:head><title>Alerts</title></svelte:head>

<div class="page">
	<div class="page-header">
		<h1 class="page-title">Alerts</h1>
		<span class="spacer"></span>
		<SegFilter
			label="Alerts view"
			bind:value={tab}
			options={[
				{ value: 'alerts', label: 'Alerts', count: activeCount },
				{ value: 'settings', label: 'Settings' }
			]}
		/>
	</div>

	{#if tab === 'alerts'}
		<div class="row">
			<SegFilter
				label="Show"
				small
				bind:value={filter}
				options={[
					{ value: 'active', label: 'Active', count: activeCount },
					{ value: 'all', label: 'All', count: list.length }
				]}
			/>
		</div>
		{#if error && !loaded}
			<ErrorState title="Can't load alerts" detail={error} onretry={load} />
		{:else if !loaded}
			{#each Array(5), i (i)}<div class="skeleton" style="height:56px"></div>{/each}
		{:else if shown.length === 0}
			<EmptyState
				icon="ph-bell-simple"
				title={filter === 'active' ? 'No active alerts' : 'No alerts yet'}
				detail="Down services, offline nodes, full disks, failed downloads, expiring certificates and new requests show up here."
			/>
		{:else}
			<ul class="card list">
				{#each shown as a (a.id)}
					{@const sev = SEVERITY[a.severity] ?? SEVERITY.info}
					<li class:resolved={!!a.resolved_at}>
						<Icon
							name={a.resolved_at ? 'ph-check-circle' : sev.icon}
							color={a.resolved_at ? 'var(--health-healthy)' : sev.color}
							size={20}
						/>
						<a class="txt" href={href(a)}>
							<span class="title">{a.title}</span>
							<span class="caption muted">{a.message}</span>
							<span class="caption faint">
								{#if a.resolved_at}Resolved after {duration(
										(new Date(a.resolved_at).getTime() - new Date(a.created_at).getTime()) / 1000
									)}{/if}
								{#each a.delivery as d (d.channel)}
									<span class="chip" class:bad={d.result === 'failed'} title={d.detail ?? ''}
										>{CHANNEL_LABEL[d.channel] ?? d.channel}: {RESULT[d.result] ?? d.result}</span
									>
								{/each}
							</span>
						</a>
						<span class="caption faint when"><RelativeTime value={a.created_at} /></span>
					</li>
				{/each}
			</ul>
		{/if}
	{:else if view}
		<section class="card pad">
			<h2 class="section-title">Channels</h2>
			<ul class="channels">
				{#each view.channels as c (c.id)}
					<li>
						<Icon name={c.id === 'ntfy' ? 'ph-paper-plane-tilt' : 'ph-device-mobile'} size={18} />
						<span class="txt"
							><span>{c.label}</span><span class="caption faint mono">{c.detail}</span></span
						>
						<span class="tag {c.configured ? 'tag-healthy' : 'tag-neutral'}"
							>{c.configured ? 'Ready' : 'Not set up'}</span
						>
						{#if owner}
							<button
								class="btn btn-secondary btn-sm"
								disabled={!c.configured}
								onclick={() => void test(c.id)}>Send test</button
							>
						{/if}
					</li>
				{/each}
			</ul>
			<p class="caption faint">
				Quiet hours use the server's time zone (UTC{view.timezone}). ntfy topics are shared, so
				mutes below don't apply to ntfy.
			</p>
		</section>

		<section class="card tablecard">
			<table class="table">
				<thead>
					<tr
						><th>Alert</th><th>On</th><th>Channels</th><th>Quiet hours</th><th
							title="Hide these alerts in this app for you">Mute for me</th
						></tr
					>
				</thead>
				<tbody>
					{#each draft as t, i (t.alert_type)}
						{@const meta = view.types[i]}
						<tr>
							<td
								><span>{meta.label}</span><span class="caption faint block">{meta.description}</span
								></td
							>
							<td
								><Switch
									bind:checked={t.enabled}
									label="{meta.label} alerts"
									disabled={!owner}
								/></td
							>
							<td class="nowrap">
								{#each view.channels as c (c.id)}
									<label class="check">
										<input
											type="checkbox"
											checked={t.channels.includes(c.id)}
											disabled={!owner || !t.enabled}
											onchange={() => toggleChannel(t, c.id)}
										/>
										{c.label}
									</label>
								{/each}
							</td>
							<td class="nowrap">
								<label class="check">
									<input
										type="checkbox"
										checked={!!t.quiet_hours}
										disabled={!owner || !t.enabled}
										onchange={(e) => setQuiet(t, e.currentTarget.checked)}
									/>
									{t.quiet_hours ? '' : 'Off'}
								</label>
								{#if t.quiet_hours}
									<input
										class="input time"
										type="time"
										aria-label="{meta.label} quiet hours start"
										bind:value={t.quiet_hours.start}
										disabled={!owner}
									/>
									–
									<input
										class="input time"
										type="time"
										aria-label="{meta.label} quiet hours end"
										bind:value={t.quiet_hours.end}
										disabled={!owner}
									/>
								{/if}
							</td>
							<td
								><Switch
									checked={mutes.includes(t.alert_type)}
									label="Mute {meta.label} for me"
									onchange={() => void toggleMute(t.alert_type)}
								/></td
							>
						</tr>
					{/each}
				</tbody>
			</table>
		</section>
		{#if owner}
			<div class="actions">
				<button class="btn btn-secondary" disabled={!dirty || saving} onclick={loadSettings}
					>Discard</button
				>
				<button class="btn btn-primary" disabled={!dirty || saving} onclick={save}>Save</button>
			</div>
		{/if}
	{/if}
</div>

<style>
	.row {
		display: flex;
	}
	.list {
		list-style: none;
		margin: 0;
		padding: 4px 0;
	}
	.list li {
		display: flex;
		align-items: flex-start;
		gap: 12px;
		padding: 10px 14px;
	}
	.list li + li {
		border-top: 1px solid var(--color-divider);
	}
	.resolved .title {
		color: var(--color-neutral-400);
	}
	.txt {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
		color: inherit;
		text-decoration: none;
	}
	a.txt:hover .title {
		text-decoration: underline;
	}
	.title {
		font-size: 14px;
	}
	.chip {
		display: inline-block;
		margin-left: 6px;
		padding: 0 6px;
		border-radius: 4px;
		background: var(--color-raised);
	}
	.chip:first-child {
		margin-left: 0;
	}
	.chip.bad {
		color: var(--health-down-text);
	}
	.when {
		white-space: nowrap;
	}
	.pad {
		padding: 16px;
	}
	.section-title {
		margin-bottom: 8px;
	}
	.channels {
		list-style: none;
		margin: 0 0 8px;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.channels li {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.tablecard {
		padding: 4px 8px;
		overflow-x: auto;
	}
	.block {
		display: block;
	}
	.nowrap {
		white-space: nowrap;
	}
	.check {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		margin-right: 10px;
		font-size: 13px;
	}
	.time {
		width: 104px;
		padding: 4px 6px;
		font-size: 13px;
	}
	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
	}
</style>
