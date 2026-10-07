<script lang="ts">
	// Pair this device (installed PWA): scan the QR shown on the desktop or type the code.
	// The backend answers with an HttpOnly device cookie, so the token is never readable
	// by scripts.
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { api, errorMessage, unwrap } from '#lib/api/client.js';
	import Icon from '#lib/components/Icon.svelte';
	import Logo from '#lib/components/Logo.svelte';
	import QrScanner from '#lib/components/QrScanner.svelte';
	import { session } from '#lib/session.svelte.js';

	function guessName(): string {
		const ua = navigator.userAgent;
		if (/iPad/.test(ua)) return 'iPad';
		if (/iPhone/.test(ua)) return 'iPhone';
		if (/Android/.test(ua)) return 'Android phone';
		return 'Browser';
	}

	let code = $state(page.url.searchParams.get('code') ?? '');
	let name = $state(guessName());
	let scanning = $state(false);
	let busy = $state(false);
	let error = $state('');

	/** QR content is `quorum://pair?server=<url>&code=<code>`; plain codes work too. */
	function onScan(text: string) {
		scanning = false;
		try {
			const url = new URL(text);
			const server = url.searchParams.get('server');
			const c = url.searchParams.get('code');
			if (!c) throw new Error();
			if (server && new URL(server).host !== location.host) {
				error = `This code is for ${new URL(server).host}. Open Quorum there to pair.`;
				return;
			}
			code = c;
		} catch {
			code = text.trim();
		}
		void submit();
	}

	async function submit(e?: SubmitEvent) {
		e?.preventDefault();
		if (!code.trim()) return;
		busy = true;
		error = '';
		try {
			unwrap(
				await api.POST('/pairing/complete', {
					body: { code: code.trim(), device_name: name.trim() || guessName(), platform: 'pwa' }
				})
			);
			const me = await session.loadMe();
			if (!me) throw new Error('Paired, but the device cookie was not accepted.');
			await session.start(me);
			await goto(resolve('/'));
		} catch (err) {
			const msg = errorMessage(err);
			error =
				msg === 'sign in required' ? 'That code is wrong or has expired. Make a new one.' : msg;
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head><title>Pair this device</title></svelte:head>

<div class="auth">
	<form class="box" onsubmit={submit}>
		<Logo size={40} />
		<h1>Pair this device</h1>
		<p class="caption muted">
			On a computer signed in as the owner, open Settings → Users and devices → Pair a device. Scan
			the QR code or type the code below.
		</p>
		{#if scanning}
			<QrScanner
				ondetect={onScan}
				onerror={(m) => {
					scanning = false;
					error = m;
				}}
			/>
			<button class="btn btn-secondary" type="button" onclick={() => (scanning = false)}
				>Cancel</button
			>
		{:else}
			<button class="btn btn-secondary big" type="button" onclick={() => (scanning = true)}
				><Icon name="ph-qr-code" size={20} />Scan QR code</button
			>
		{/if}
		<div class="field">
			<label for="code">Code</label>
			<input
				id="code"
				class="input mono code"
				autocomplete="one-time-code"
				autocapitalize="characters"
				spellcheck="false"
				placeholder="XXXX-XXXX"
				required
				bind:value={code}
			/>
		</div>
		<div class="field">
			<label for="name">Device name</label>
			<input id="name" class="input" maxlength="64" required bind:value={name} />
		</div>
		{#if error}<p class="error" role="alert">{error}</p>{/if}
		<button class="btn btn-primary big" type="submit" disabled={busy}>
			{busy ? 'Pairing…' : 'Pair'}
		</button>
		<a class="caption" href={resolve('/login')}>Sign in with a password instead</a>
	</form>
</div>

<style>
	.auth {
		min-height: 100dvh;
		display: grid;
		place-items: center;
		padding: max(16px, env(safe-area-inset-top)) 16px max(16px, env(safe-area-inset-bottom));
		background: radial-gradient(60% 40% at 50% 40%, #1e2142 0%, #161826 70%);
	}
	.box {
		width: min(380px, 100%);
		background: var(--color-surface);
		border-radius: var(--radius-lg);
		box-shadow: var(--shadow-sm);
		padding: 24px;
		display: flex;
		flex-direction: column;
		gap: 14px;
	}
	h1 {
		font: var(--text-title);
	}
	.big {
		min-height: 44px;
		justify-content: center;
	}
	.code {
		font-size: 18px;
		letter-spacing: 0.08em;
		text-align: center;
	}
	.error {
		color: var(--health-down);
		font-size: 13px;
	}
</style>
