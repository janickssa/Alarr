<script lang="ts">
	import { api, errorMessage, unwrap } from '#lib/api/client.js';
	import { session } from '#lib/session.svelte.js';
	import { toasts } from '#lib/toasts.svelte.js';

	let current = $state('');
	let next = $state('');
	let repeat = $state('');
	let error = $state('');

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		if (next !== repeat) {
			error = 'The new passwords do not match.';
			return;
		}
		try {
			unwrap(
				await api.PUT('/auth/password', { body: { current_password: current, new_password: next } })
			);
			current = next = repeat = '';
			toasts.push({
				kind: 'success',
				title: 'Password changed',
				sub: 'Your other web sessions were signed out.'
			});
		} catch (err) {
			const m = errorMessage(err);
			error = m === 'not allowed for your role' ? 'The current password is wrong.' : m;
		}
	}
</script>

<svelte:head><title>Account</title></svelte:head>

<form class="card narrow" onsubmit={submit}>
	<h2 class="section-title">Change password</h2>
	<p class="caption muted">Signed in as {session.me?.user.username} ({session.me?.user.role}).</p>
	<div class="field">
		<label for="cur">Current password</label>
		<input
			id="cur"
			class="input"
			type="password"
			required
			autocomplete="current-password"
			bind:value={current}
		/>
	</div>
	<div class="field">
		<label for="new">New password</label>
		<input
			id="new"
			class="input"
			type="password"
			minlength="8"
			required
			autocomplete="new-password"
			bind:value={next}
		/>
	</div>
	<div class="field">
		<label for="rep">Repeat new password</label>
		<input
			id="rep"
			class="input"
			type="password"
			required
			autocomplete="new-password"
			bind:value={repeat}
		/>
	</div>
	{#if error}<p class="error" role="alert">{error}</p>{/if}
	<div class="row">
		<span class="spacer"></span><button class="btn btn-primary" type="submit"
			>Change password</button
		>
	</div>
</form>

<style>
	.narrow {
		max-width: 480px;
	}
	.error {
		color: var(--health-down);
		font-size: 13px;
	}
</style>
