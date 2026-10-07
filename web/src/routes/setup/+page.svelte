<script lang="ts">
	// First run: setup token → owner account → add first services.
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { onMount } from 'svelte';
	import { api, errorMessage, unwrap } from '#lib/api/client.js';
	import Icon from '#lib/components/Icon.svelte';
	import Logo from '#lib/components/Logo.svelte';
	import { session } from '#lib/session.svelte.js';

	let step = $state<1 | 2>(1);
	let token = $state('');
	let username = $state('');
	let password = $state('');
	let confirm = $state('');
	let error = $state('');
	let busy = $state(false);

	onMount(async () => {
		const res = await api.GET('/auth/setup').catch(() => null);
		if (res?.data && !res.data.needs_setup && !session.me) await goto(resolve('/login'));
	});

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		if (password !== confirm) {
			error = 'The passwords do not match.';
			return;
		}
		busy = true;
		try {
			const me = unwrap(
				await api.POST('/auth/setup', { body: { setup_token: token.trim(), username, password } })
			);
			await session.start(me);
			step = 2;
		} catch (err) {
			error =
				errorMessage(err) === 'not allowed for your role'
					? 'That setup token is not valid. Copy it from the server log.'
					: errorMessage(err);
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head><title>Set up</title></svelte:head>

<div class="auth">
	<div class="box">
		<Logo size={40} />
		<ol class="steps" aria-label="Setup steps">
			<li aria-current={step === 1 ? 'step' : undefined} class:done={step > 1}>
				1 · Owner account
			</li>
			<li aria-current={step === 2 ? 'step' : undefined}>2 · First services</li>
		</ol>
		{#if step === 1}
			<form onsubmit={submit}>
				<h1>Create the owner account</h1>
				<p class="muted">
					The server printed a one-time setup token in its log (<span class="mono"
						>journalctl -u quorum | grep 'setup token'</span
					>).
				</p>
				<div class="field">
					<label for="t">Setup token</label>
					<input id="t" class="input mono" required autocomplete="off" bind:value={token} />
				</div>
				<div class="field">
					<label for="u">Username</label>
					<input id="u" class="input" required autocomplete="username" bind:value={username} />
				</div>
				<div class="field">
					<label for="p">Password</label>
					<input
						id="p"
						class="input"
						type="password"
						minlength="8"
						required
						autocomplete="new-password"
						bind:value={password}
					/>
					<span class="hint">At least 8 characters.</span>
				</div>
				<div class="field">
					<label for="c">Repeat password</label>
					<input
						id="c"
						class="input"
						type="password"
						required
						autocomplete="new-password"
						bind:value={confirm}
					/>
				</div>
				{#if error}<p class="error" role="alert">{error}</p>{/if}
				<button class="btn btn-primary" type="submit" disabled={busy}>Create account</button>
			</form>
		{:else}
			<h1>Add your first services</h1>
			<p class="muted">You can add and change services any time in Settings.</p>
			<a class="choice" href="{resolve('/settings/services/new')}?preset=proxmox">
				<Icon name="ph-hard-drives" size={22} />
				<span
					><strong>Proxmox cluster</strong><span class="muted">Nodes, guests and power actions</span
					></span
				>
			</a>
			<a class="choice" href={resolve('/settings/services/new')}>
				<Icon name="ph-heartbeat" size={22} />
				<span
					><strong>Health checks</strong><span class="muted"
						>Vaultwarden, BookStack, any URL or port</span
					></span
				>
			</a>
			<a class="btn btn-secondary" href={resolve('/')}>Skip for now</a>
		{/if}
	</div>
</div>

<style>
	.auth {
		min-height: 100vh;
		display: grid;
		place-items: center;
		padding: 16px;
		background: radial-gradient(60% 40% at 50% 40%, #1e2142 0%, #161826 70%);
	}
	.box,
	form {
		display: flex;
		flex-direction: column;
		gap: 16px;
	}
	.box {
		width: min(440px, 100%);
		background: var(--color-surface);
		border-radius: var(--radius-lg);
		box-shadow: var(--shadow-sm);
		padding: 28px;
	}
	h1 {
		font: var(--text-title);
	}
	.steps {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		gap: 16px;
		font-size: 12px;
		color: var(--color-neutral-500);
	}
	.steps [aria-current='step'] {
		color: var(--color-accent-300);
	}
	.steps .done {
		color: var(--color-neutral-300);
	}
	.error {
		color: var(--health-down);
		font-size: 13px;
	}
	.choice {
		display: flex;
		align-items: center;
		gap: 14px;
		padding: 14px;
		border-radius: var(--radius-md);
		box-shadow: 0 0 0 1px var(--color-divider);
		color: var(--color-text);
		text-decoration: none;
	}
	.choice:hover {
		box-shadow: 0 0 0 1px var(--color-accent);
		color: var(--color-text);
	}
	.choice > span {
		display: flex;
		flex-direction: column;
		gap: 2px;
		font-size: 13px;
	}
	.choice strong {
		font-weight: 500;
		font-size: 14px;
	}
</style>
