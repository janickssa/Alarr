<script lang="ts">
	// Add or configure a service: kind-specific fields, linked container, connection test
	// (with fingerprint pinning for self-signed certificates), save and delete.
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { onMount, untrack } from 'svelte';
	import {
		api,
		errorMessage,
		unwrap,
		type CatalogueEntry,
		type ConnectionTest,
		type Guest,
		type ServiceView
	} from '#lib/api/client.js';
	import { guestId } from '#lib/format.js';
	import { session } from '#lib/session.svelte.js';
	import { toasts } from '#lib/toasts.svelte.js';
	import Dialog from '../Dialog.svelte';
	import Icon from '../Icon.svelte';
	import SegFilter from '../SegFilter.svelte';
	import ServiceIcon from '../ServiceIcon.svelte';

	let { entry, existing }: { entry: CatalogueEntry; existing?: ServiceView } = $props();

	// The form is keyed by service, so it only needs the initial values.
	const { entry: e0, existing: x0 } = untrack(() => ({ entry, existing }));
	type Settings = Record<string, string | number | undefined | null>;
	const s0 = (x0?.settings ?? {}) as Settings;
	const kind = e0.kind ?? 'health_check';

	let name = $state(x0?.name ?? (e0.preset === 'custom' ? '' : e0.name));
	let baseUrl = $state(x0?.base_url ?? '');
	let linkedVmid = $state<number | null>(x0?.linked_vmid ?? e0.demo_vmid ?? null);
	// Proxmox
	let tokenId = $state((s0.token_id as string) ?? 'quorum@pve!quorum');
	let tokenSecret = $state('');
	let fingerprint = $state((s0.fingerprint as string) ?? '');
	// Health check
	let check = $state<'http' | 'tcp'>((s0.check as 'http' | 'tcp') ?? 'http');
	let path = $state((s0.path as string) ?? e0.health_path ?? '');
	let expected = $state(s0.expected_status != null ? String(s0.expected_status) : '');
	let address = $state((s0.address as string) ?? '');
	let tlsFingerprint = $state((s0.tls_fingerprint as string) ?? '');
	let timeoutMs = $state(s0.timeout_ms != null ? String(s0.timeout_ms) : '');
	let slowMs = $state(s0.slow_ms != null ? String(s0.slow_ms) : '');

	let guests = $state<Guest[]>([]);
	let test = $state<ConnectionTest | null>(null);
	let testing = $state(false);
	let saving = $state(false);
	let error = $state('');
	let deleteOpen = $state(false);

	const secretSet = $derived(existing?.secrets?.token_secret?.set ?? false);

	onMount(async () => {
		if (session.proxmox) {
			const res = await api.GET('/proxmox/guests');
			guests = (res.data ?? []).filter((g) => !g.template);
		}
		// Keep the catalogue's container only if it exists here.
		if (!existing && linkedVmid != null && !guests.some((g) => g.vmid === linkedVmid))
			linkedVmid = null;
	});

	const num = (v: string) => (v.trim() === '' ? null : Number(v));

	function settings(): Record<string, unknown> {
		if (kind === 'proxmox')
			return { token_id: tokenId.trim(), fingerprint: fingerprint.trim() || null };
		return check === 'http'
			? {
					check,
					path: path.trim() || null,
					expected_status: num(expected),
					tls_fingerprint: tlsFingerprint.trim() || null,
					timeout_ms: num(timeoutMs),
					slow_ms: num(slowMs),
					address: null
				}
			: { check, address: address.trim(), timeout_ms: num(timeoutMs), slow_ms: num(slowMs) };
	}

	/** Drop null keys for create requests (PATCH uses null to remove). */
	function compact(o: Record<string, unknown>) {
		return Object.fromEntries(Object.entries(o).filter(([, v]) => v != null));
	}

	function secrets(): Record<string, string> | undefined {
		return kind === 'proxmox' && tokenSecret ? { token_secret: tokenSecret } : undefined;
	}

	function createBody() {
		return {
			kind,
			preset: entry.preset,
			name: name.trim(),
			base_url: kind === 'health_check' && check === 'tcp' ? null : baseUrl.trim() || null,
			settings: compact(settings()),
			secrets: secrets(),
			linked_vmid: linkedVmid
		};
	}

	function updateBody() {
		return {
			name: name.trim(),
			base_url: kind === 'health_check' && check === 'tcp' ? null : baseUrl.trim() || null,
			settings: settings(),
			secrets: secrets(),
			linked_vmid: linkedVmid
		};
	}

	async function runTest() {
		testing = true;
		test = null;
		error = '';
		try {
			test = existing
				? unwrap(
						await api.POST('/services/{id}/test', {
							params: { path: { id: existing.id } },
							body: updateBody()
						})
					)
				: unwrap(await api.POST('/services/test', { body: createBody() }));
		} catch (e) {
			error = errorMessage(e);
		} finally {
			testing = false;
		}
	}

	async function save(e: SubmitEvent) {
		e.preventDefault();
		saving = true;
		error = '';
		try {
			const saved = existing
				? unwrap(
						await api.PATCH('/services/{id}', {
							params: { path: { id: existing.id } },
							body: updateBody()
						})
					)
				: unwrap(await api.POST('/services', { body: createBody() }));
			await session.loadServices();
			toasts.push({ kind: 'success', title: `${saved.name} ${existing ? 'saved' : 'added'}` });
			await goto(resolve('/settings/services'));
		} catch (err) {
			error = errorMessage(err);
		} finally {
			saving = false;
		}
	}

	async function remove() {
		if (!existing) return;
		deleteOpen = false;
		try {
			unwrap(await api.DELETE('/services/{id}', { params: { path: { id: existing.id } } }));
			await session.loadServices();
			toasts.push({ kind: 'success', title: `${existing.name} removed` });
			await goto(resolve('/settings/services'));
		} catch (e) {
			error = errorMessage(e);
		}
	}

	function pin(fp: string) {
		if (kind === 'proxmox') fingerprint = fp;
		else tlsFingerprint = fp;
		test = null;
	}
</script>

<form class="form" onsubmit={save}>
	<div class="head">
		<ServiceIcon icon={entry.icon} hue={entry.accent_hue} chroma={entry.accent_chroma} size={40} />
		<div>
			<h1 class="page-title">{existing ? existing.name : `Add ${entry.name}`}</h1>
			<span class="caption muted"
				>{kind === 'proxmox' ? 'Proxmox VE API' : 'Health check'} · {entry.level}</span
			>
		</div>
	</div>

	<section class="card">
		<div class="field">
			<label for="name">Name</label>
			<input
				id="name"
				class="input"
				required
				maxlength="80"
				bind:value={name}
				placeholder={entry.name}
			/>
		</div>
		{#if kind === 'health_check'}
			<div class="field">
				<span class="label" id="check-l">Check</span>
				<SegFilter
					label="Check type"
					bind:value={check}
					options={[
						{ value: 'http', label: 'HTTP(S) request' },
						{ value: 'tcp', label: 'TCP port' }
					]}
				/>
			</div>
		{/if}
		{#if kind === 'proxmox' || check === 'http'}
			<div class="field">
				<label for="url">Base URL</label>
				<input
					id="url"
					class="input mono"
					required
					bind:value={baseUrl}
					placeholder={kind === 'proxmox' ? 'https://10.0.20.11:8006' : 'https://vault.example.lan'}
				/>
				<span class="hint"
					>{kind === 'proxmox'
						? 'Any node of the cluster, port 8006.'
						: 'Also the "Open web UI" link.'}</span
				>
			</div>
		{/if}
	</section>

	{#if kind === 'proxmox'}
		<section class="card">
			<h2 class="overline">API token</h2>
			<p class="caption muted">
				Create a token with only VM.Audit, Sys.Audit, Datastore.Audit and VM.PowerMgmt. See
				<span class="mono">deploy/README.md</span>.
			</p>
			<div class="field">
				<label for="tid">Token ID</label>
				<input
					id="tid"
					class="input mono"
					required
					bind:value={tokenId}
					placeholder="quorum@pve!quorum"
				/>
			</div>
			<div class="field">
				<label for="ts">Token secret</label>
				<input
					id="ts"
					class="input mono"
					type="password"
					autocomplete="off"
					required={!secretSet}
					bind:value={tokenSecret}
					placeholder={secretSet ? '•••• (set, leave empty to keep)' : '00000000-0000-…'}
				/>
				<span class="hint">Stored encrypted. Never shown again.</span>
			</div>
			<div class="field">
				<label for="fp">Certificate fingerprint (SHA-256)</label>
				<input id="fp" class="input mono" bind:value={fingerprint} placeholder="AB:CD:…" />
				<span class="hint">
					For the self-signed certificate. On the node: <span class="mono"
						>openssl x509 -in /etc/pve/local/pve-ssl.pem -noout -fingerprint -sha256</span
					>
				</span>
			</div>
		</section>
	{:else}
		<section class="card">
			<h2 class="overline">Check details</h2>
			{#if check === 'http'}
				<div class="cols">
					<div class="field">
						<label for="path">Path</label>
						<input id="path" class="input mono" bind:value={path} placeholder="/" />
					</div>
					<div class="field">
						<label for="exp">Expected status</label>
						<input
							id="exp"
							class="input mono"
							inputmode="numeric"
							bind:value={expected}
							placeholder="any 2xx or 3xx"
						/>
					</div>
				</div>
				<div class="field">
					<label for="tfp">Certificate fingerprint (optional)</label>
					<input
						id="tfp"
						class="input mono"
						bind:value={tlsFingerprint}
						placeholder="Only for self-signed HTTPS"
					/>
				</div>
			{:else}
				<div class="field">
					<label for="addr">Address</label>
					<input
						id="addr"
						class="input mono"
						required
						bind:value={address}
						placeholder="10.0.20.111:445"
					/>
				</div>
			{/if}
			<div class="cols">
				<div class="field">
					<label for="to">Timeout (ms)</label>
					<input
						id="to"
						class="input mono"
						inputmode="numeric"
						bind:value={timeoutMs}
						placeholder="5000"
					/>
				</div>
				<div class="field">
					<label for="slow">Warn when slower than (ms)</label>
					<input
						id="slow"
						class="input mono"
						inputmode="numeric"
						bind:value={slowMs}
						placeholder="2000"
					/>
				</div>
			</div>
		</section>
	{/if}

	{#if session.proxmox}
		<section class="card">
			<h2 class="overline">Linked container</h2>
			<div class="field">
				<label for="vm">Container or VM</label>
				<select
					id="vm"
					class="input"
					value={linkedVmid ?? ''}
					onchange={(e) => {
						const v = (e.currentTarget as HTMLSelectElement).value;
						linkedVmid = v ? Number(v) : null;
					}}
				>
					<option value="">None</option>
					{#each guests as g (g.vmid)}
						<option value={g.vmid}>{guestId(g.guest_type, g.vmid)} · {g.name} · {g.node}</option>
					{/each}
				</select>
				<span class="hint"
					>Shows the container and its power menu on the service page. A Proxmox tag <span
						class="mono">svc-&lt;id&gt;</span
					> also links it.</span
				>
			</div>
		</section>
	{/if}

	{#if test}
		<section class="card result" class:ok={test.ok} role="status">
			<div class="row">
				<Icon
					name={test.ok ? 'ph-check-circle' : 'ph-x-circle'}
					fill
					color={test.ok ? 'var(--health-healthy)' : 'var(--health-down)'}
					size={18}
				/>
				<strong>{test.ok ? 'Connected' : 'Connection failed'}</strong>
				<span class="muted">{test.message}</span>
				{#if test.version}<span class="mono faint">v{test.version}</span>{/if}
			</div>
			{#each test.warnings as w (w)}
				<div class="row caption warn">
					<Icon name="ph-warning" fill color="var(--health-warning)" />{w}
				</div>
			{/each}
			{#if test.presented_fingerprint}
				<div class="fp">
					<span class="caption"
						>The server presented this certificate. Compare it with the fingerprint on the host
						before pinning it:</span
					>
					<code class="mono">{test.presented_fingerprint}</code>
					<button
						type="button"
						class="btn btn-secondary btn-sm"
						onclick={() => pin(test?.presented_fingerprint ?? '')}
					>
						<Icon name="ph-push-pin" />Pin this fingerprint
					</button>
				</div>
			{/if}
		</section>
	{/if}

	{#if error}<p class="error" role="alert">{error}</p>{/if}

	<div class="row buttons">
		{#if existing}
			<button type="button" class="btn btn-danger" onclick={() => (deleteOpen = true)}
				><Icon name="ph-trash" />Remove</button
			>
		{/if}
		<span class="spacer"></span>
		<button type="button" class="btn btn-secondary" disabled={testing} onclick={runTest}>
			<Icon name={testing ? 'ph-circle-notch' : 'ph-plugs-connected'} spin={testing} />Test
			connection
		</button>
		<a class="btn btn-secondary" href={resolve('/settings/services')}>Cancel</a>
		<button class="btn btn-primary" type="submit" disabled={saving}
			>{existing ? 'Save' : 'Add service'}</button
		>
	</div>
</form>

<Dialog bind:open={deleteOpen} title="Remove {existing?.name}?" destructive>
	<div class="dlg-title">Remove {existing?.name}?</div>
	<p class="caption muted">
		Quorum stops checking it and forgets its settings and secrets. Its history stays in the activity
		log.
	</p>
	{#snippet actions()}
		<button class="btn btn-secondary" onclick={() => (deleteOpen = false)}>Cancel</button>
		<button class="btn btn-danger" onclick={remove}><Icon name="ph-trash" />Remove</button>
	{/snippet}
</Dialog>

<style>
	.form {
		display: flex;
		flex-direction: column;
		gap: var(--gap);
		max-width: 720px;
	}
	.head {
		display: flex;
		align-items: center;
		gap: 14px;
	}
	.cols {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 12px;
	}
	.result {
		box-shadow: 0 0 0 1px var(--health-down-edge);
	}
	.result.ok {
		box-shadow: 0 0 0 1px var(--health-healthy-fill);
	}
	.result .row {
		flex-wrap: wrap;
	}
	.warn {
		color: var(--health-warning);
	}
	.fp {
		display: flex;
		flex-direction: column;
		gap: 6px;
		align-items: flex-start;
	}
	code {
		font-size: 12px;
		word-break: break-all;
		padding: 6px 8px;
		background: var(--color-bg);
		border-radius: var(--radius-sm);
	}
	.buttons {
		gap: 8px;
		flex-wrap: wrap;
	}
	.error {
		color: var(--health-down);
		font-size: 13px;
	}
	.dlg-title {
		font-size: 18px;
		font-weight: 500;
	}
</style>
