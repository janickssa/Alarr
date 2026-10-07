<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { onMount } from 'svelte';
	import { api, errorMessage, unwrap } from '#lib/api/client.js';
	import Banner from '#lib/components/Banner.svelte';
	import Logo from '#lib/components/Logo.svelte';
	import { session } from '#lib/session.svelte.js';

	let username = $state('');
	let password = $state('');
	let error = $state('');
	let busy = $state(false);
	let demo = $state(false);

	onMount(async () => {
		const res = await api.GET('/auth/setup').catch(() => null);
		if (res?.data?.needs_setup) await goto(resolve('/setup'));
		demo = res?.data?.demo ?? false;
	});

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		error = '';
		try {
			const me = unwrap(await api.POST('/auth/login', { body: { username, password } }));
			await session.start(me);
			await goto(resolve('/'));
		} catch (err) {
			const msg = errorMessage(err);
			error = msg === 'sign in required' ? 'Wrong username or password.' : msg;
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head><title>Sign in</title></svelte:head>

<div class="auth">
	<form class="box" onsubmit={submit}>
		<Logo size={40} />
		<h1>Sign in</h1>
		{#if demo}
			<Banner tone="progress" text="Demo mode: sign in as demo / demo (owner) or guest / guest." />
		{/if}
		<div class="field">
			<label for="u">Username</label>
			<input id="u" class="input" autocomplete="username" required bind:value={username} />
		</div>
		<div class="field">
			<label for="p">Password</label>
			<input
				id="p"
				class="input"
				type="password"
				autocomplete="current-password"
				required
				bind:value={password}
			/>
		</div>
		{#if error}<p class="error" role="alert">{error}</p>{/if}
		<button class="btn btn-primary" type="submit" disabled={busy}>
			{busy ? 'Signing in…' : 'Sign in'}
		</button>
	</form>
</div>

<style>
	.auth {
		min-height: 100vh;
		display: grid;
		place-items: center;
		padding: 16px;
		background: radial-gradient(60% 40% at 50% 40%, #1e2142 0%, #161826 70%);
	}
	.box {
		width: min(380px, 100%);
		background: var(--color-surface);
		border-radius: var(--radius-lg);
		box-shadow: var(--shadow-sm);
		padding: 28px;
		display: flex;
		flex-direction: column;
		gap: 16px;
	}
	h1 {
		font: var(--text-title);
	}
	.error {
		color: var(--health-down);
		font-size: 13px;
	}
</style>
