<script lang="ts">
	// Users (roles, guest access) and paired devices, with device pairing by code or QR.
	import QRCode from 'qrcode';
	import { onMount } from 'svelte';
	import {
		api,
		errorMessage,
		unwrap,
		type Device,
		type Role,
		type Schemas,
		type User
	} from '#lib/api/client.js';
	import Dialog from '#lib/components/Dialog.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import RelativeTime from '#lib/components/RelativeTime.svelte';
	import { date } from '#lib/format.js';
	import { session } from '#lib/session.svelte.js';
	import { toasts } from '#lib/toasts.svelte.js';

	let users = $state<User[]>([]);
	let devices = $state<Device[]>([]);

	// Add / edit user
	let userOpen = $state(false);
	let editing = $state<User | null>(null);
	let fUsername = $state('');
	let fPassword = $state('');
	let fRole = $state<Role>('member');
	let fAllowed = $state<string[]>([]);
	let fCanAct = $state(false);
	let fError = $state('');

	// Pairing
	let pairOpen = $state(false);
	let pairing = $state<Schemas['PairingStarted'] | null>(null);
	let pairFor = $state<number | null>(null);
	let qr = $state('');
	let remaining = $state(0);
	let deviceCountAtStart = 0;

	// Confirmations
	let confirmUser = $state<User | null>(null);
	let confirmDevice = $state<Device | null>(null);

	async function load() {
		const [u, d] = await Promise.all([api.GET('/users'), api.GET('/devices')]);
		users = u.data ?? [];
		devices = d.data ?? [];
	}
	onMount(load);

	const userName = (id: number) => users.find((u) => u.id === id)?.username ?? `#${id}`;

	function openUser(u: User | null) {
		editing = u;
		fUsername = u?.username ?? '';
		fPassword = '';
		fRole = u?.role ?? 'member';
		fAllowed = [...(u?.allowed_services ?? [])];
		fCanAct = u?.can_act ?? false;
		fError = '';
		userOpen = true;
	}

	async function saveUser(e: SubmitEvent) {
		e.preventDefault();
		fError = '';
		try {
			if (editing) {
				unwrap(
					await api.PUT('/users/{id}', {
						params: { path: { id: editing.id } },
						body: {
							role: fRole,
							allowed_services: fAllowed,
							can_act: fCanAct,
							password: fPassword || null
						}
					})
				);
			} else {
				unwrap(
					await api.POST('/users', {
						body: {
							username: fUsername,
							password: fPassword,
							role: fRole,
							allowed_services: fAllowed,
							can_act: fCanAct
						}
					})
				);
			}
			userOpen = false;
			await load();
		} catch (err) {
			fError = errorMessage(err);
		}
	}

	async function deleteUser() {
		const u = confirmUser;
		confirmUser = null;
		if (!u) return;
		try {
			unwrap(await api.DELETE('/users/{id}', { params: { path: { id: u.id } } }));
			await load();
		} catch (e) {
			toasts.push({ kind: 'error', title: `Couldn't remove ${u.username}`, sub: errorMessage(e) });
		}
	}

	async function revoke() {
		const d = confirmDevice;
		confirmDevice = null;
		if (!d) return;
		try {
			unwrap(await api.DELETE('/devices/{id}', { params: { path: { id: d.id } } }));
			toasts.push({ kind: 'success', title: `${d.name} revoked` });
			await load();
		} catch (e) {
			toasts.push({ kind: 'error', title: `Couldn't revoke ${d.name}`, sub: errorMessage(e) });
		}
	}

	async function startPairing() {
		try {
			pairing = unwrap(await api.POST('/pairing/start', { body: { user_id: pairFor } }));
			qr = await QRCode.toDataURL(pairing.qr_payload, {
				margin: 1,
				width: 220,
				color: { dark: '#161826', light: '#e9e9ed' }
			});
			deviceCountAtStart = devices.length;
			pairOpen = true;
		} catch (e) {
			toasts.push({ kind: 'error', title: "Couldn't start pairing", sub: errorMessage(e) });
		}
	}

	// While the pairing dialog is open: count down and watch for the new device.
	$effect(() => {
		if (!pairOpen || !pairing) return;
		const expires = new Date(pairing.expires_at).getTime();
		const tick = setInterval(async () => {
			remaining = Math.max(0, Math.round((expires - Date.now()) / 1000));
			const d = await api.GET('/devices');
			if (d.data && d.data.length > deviceCountAtStart) {
				devices = d.data;
				const added = d.data[d.data.length - 1];
				pairOpen = false;
				toasts.push({ kind: 'success', title: `${added.name} paired` });
			}
		}, 2000);
		remaining = Math.max(0, Math.round((expires - Date.now()) / 1000));
		return () => clearInterval(tick);
	});
</script>

<svelte:head><title>Users and devices</title></svelte:head>

<section class="sec">
	<div class="row">
		<h2 class="section-title">Users</h2>
		<span class="spacer"></span>
		<button class="btn btn-primary" onclick={() => openUser(null)}
			><Icon name="ph-user-plus" />Add user</button
		>
	</div>
	<div class="card tablecard">
		<table class="table">
			<thead
				><tr
					><th>Username</th><th>Role</th><th class="hide-sm">Access</th><th class="hide-sm"
						>Added</th
					><th></th></tr
				></thead
			>
			<tbody>
				{#each users as u (u.id)}
					<tr>
						<td
							>{u.username}{#if u.id === session.me?.user.id}<span class="faint">
									(you)</span
								>{/if}</td
						>
						<td
							><span class="tag {u.role === 'owner' ? 'tag-accent' : 'tag-neutral'}">{u.role}</span
							></td
						>
						<td class="muted caption hide-sm">
							{#if u.role === 'guest'}
								{u.allowed_services.length
									? u.allowed_services.join(', ')
									: 'No services'}{u.can_act ? ' · can run actions' : ' · view only'}
							{:else if u.role === 'member'}All services · can run actions{:else}Everything{/if}
						</td>
						<td class="caption muted hide-sm">{date(u.created_at)}</td>
						<td class="right">
							<button class="btn btn-ghost btn-sm" onclick={() => openUser(u)}>Edit</button>
							{#if u.id !== session.me?.user.id}
								<button class="btn btn-ghost btn-sm danger" onclick={() => (confirmUser = u)}
									>Remove</button
								>
							{/if}
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
</section>

<section class="sec">
	<div class="row wrap">
		<h2 class="section-title">Devices</h2>
		<span class="spacer"></span>
		<label class="caption muted" for="pairfor">Pair for</label>
		<select
			id="pairfor"
			class="input pairfor"
			onchange={(e) => (pairFor = Number((e.currentTarget as HTMLSelectElement).value) || null)}
		>
			<option value="">Me</option>
			{#each users.filter((u) => u.id !== session.me?.user.id) as u (u.id)}<option value={u.id}
					>{u.username}</option
				>{/each}
		</select>
		<button class="btn btn-primary" onclick={startPairing}
			><Icon name="ph-qr-code" />Pair a device</button
		>
	</div>
	<div class="card tablecard">
		{#if devices.length === 0}
			<p class="muted caption pad">
				No paired devices. Pair the iPhone app or the PWA by scanning a QR code or typing a code.
			</p>
		{:else}
			<table class="table">
				<thead
					><tr
						><th>Device</th><th class="hide-sm">Platform</th><th>User</th><th class="hide-sm"
							>Paired</th
						><th>Last seen</th><th></th></tr
					></thead
				>
				<tbody>
					{#each devices as d (d.id)}
						<tr>
							<td>{d.name}</td>
							<td class="mono muted hide-sm">{d.platform}{d.push ? ' · push' : ''}</td>
							<td>{userName(d.user_id)}</td>
							<td class="caption muted hide-sm">{date(d.created_at)}</td>
							<td class="caption muted"><RelativeTime value={d.last_seen_at} /></td>
							<td class="right"
								><button class="btn btn-ghost btn-sm danger" onclick={() => (confirmDevice = d)}
									>Revoke</button
								></td
							>
						</tr>
					{/each}
				</tbody>
			</table>
		{/if}
	</div>
</section>

<Dialog bind:open={userOpen} title={editing ? `Edit ${editing.username}` : 'Add user'}>
	<form class="uform" onsubmit={saveUser} id="userform">
		<div class="dlg-title">{editing ? `Edit ${editing.username}` : 'Add user'}</div>
		{#if !editing}
			<div class="field">
				<label for="un">Username</label>
				<input id="un" class="input" required bind:value={fUsername} autocomplete="off" />
			</div>
		{/if}
		<div class="field">
			<label for="pw">{editing ? 'New password (optional)' : 'Password'}</label>
			<input
				id="pw"
				class="input"
				type="password"
				minlength="8"
				required={!editing}
				bind:value={fPassword}
				autocomplete="new-password"
			/>
		</div>
		<div class="field">
			<label for="role">Role</label>
			<select id="role" class="input" bind:value={fRole}>
				<option value="owner">Owner: everything</option>
				<option value="member">Member: all services, actions, no settings</option>
				<option value="guest">Guest: only chosen services</option>
			</select>
		</div>
		{#if fRole === 'guest'}
			<fieldset class="field">
				<legend class="label">Services this guest can see</legend>
				<div class="checks">
					{#each session.services as s (s.id)}
						<label class="checkbox"
							><input type="checkbox" value={s.id} bind:group={fAllowed} />{s.name}</label
						>
					{/each}
				</div>
			</fieldset>
			<label class="checkbox"
				><input type="checkbox" bind:checked={fCanAct} />Can run power and service actions on these</label
			>
		{/if}
		{#if fError}<p class="error" role="alert">{fError}</p>{/if}
	</form>
	{#snippet actions()}
		<button class="btn btn-secondary" onclick={() => (userOpen = false)}>Cancel</button>
		<button class="btn btn-primary" type="submit" form="userform"
			>{editing ? 'Save' : 'Add user'}</button
		>
	{/snippet}
</Dialog>

<Dialog bind:open={pairOpen} title="Pair a device" width={420}>
	<div class="dlg-title">Pair a device</div>
	{#if pairing}
		<div class="pair">
			{#if qr}<img src={qr} alt="Pairing QR code" width="220" height="220" />{/if}
			<p class="caption muted">
				Scan with the Quorum app, or open <span class="mono">{location.host}/pair</span> on the phone
				and scan or type this code:
			</p>
			<span class="code mono">{pairing.code}</span>
			<span class="caption faint">
				{remaining > 0
					? `Expires in ${Math.floor(remaining / 60)}:${String(remaining % 60).padStart(2, '0')}`
					: 'Expired'} · single use
			</span>
		</div>
	{/if}
	{#snippet actions()}
		{#if remaining === 0}<button class="btn btn-secondary" onclick={startPairing}>New code</button
			>{/if}
		<button class="btn btn-secondary" onclick={() => (pairOpen = false)}>Close</button>
	{/snippet}
</Dialog>

<Dialog
	open={confirmUser != null}
	title="Remove {confirmUser?.username}?"
	destructive
	onclose={() => (confirmUser = null)}
>
	<div class="dlg-title">Remove {confirmUser?.username}?</div>
	<p class="caption muted">Their sessions and paired devices stop working immediately.</p>
	{#snippet actions()}
		<button class="btn btn-secondary" onclick={() => (confirmUser = null)}>Cancel</button>
		<button class="btn btn-danger" onclick={deleteUser}>Remove</button>
	{/snippet}
</Dialog>

<Dialog
	open={confirmDevice != null}
	title="Revoke {confirmDevice?.name}?"
	destructive
	onclose={() => (confirmDevice = null)}
>
	<div class="dlg-title">Revoke {confirmDevice?.name}?</div>
	<p class="caption muted">The device is signed out and needs to be paired again.</p>
	{#snippet actions()}
		<button class="btn btn-secondary" onclick={() => (confirmDevice = null)}>Cancel</button>
		<button class="btn btn-danger" onclick={revoke}>Revoke</button>
	{/snippet}
</Dialog>

<style>
	.sec {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}
	.wrap {
		flex-wrap: wrap;
	}
	.tablecard {
		padding: 4px 8px;
		overflow-x: auto;
	}
	.pad {
		padding: 10px 6px;
	}
	.danger {
		color: var(--health-down);
	}
	.pairfor {
		width: auto;
		min-width: 120px;
	}
	.uform {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	fieldset {
		border: 0;
		padding: 0;
		margin: 0;
	}
	.checks {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 6px;
		max-height: 200px;
		overflow: auto;
		padding: 4px 0;
	}
	.dlg-title {
		font-size: 18px;
		font-weight: 500;
	}
	.pair {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 10px;
		text-align: center;
	}
	.pair img {
		border-radius: var(--radius-md);
	}
	.code {
		font-size: 28px;
		letter-spacing: 0.12em;
	}
	.error {
		color: var(--health-down);
		font-size: 13px;
	}
</style>
