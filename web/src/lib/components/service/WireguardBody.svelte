<script lang="ts">
	// WireGuard (read-only, via the Quorum agent): peers, last handshake, transfer and online
	// state (handshake within 3 min), or install steps when no agent answers.
	import type { WireguardData } from '#lib/api/client.js';
	import { bytes } from '#lib/format.js';
	import EmptyState from '../EmptyState.svelte';
	import HealthDot from '../HealthDot.svelte';
	import RelativeTime from '../RelativeTime.svelte';
	import StatTile from '../StatTile.svelte';

	let { data }: { data: WireguardData } = $props();
	const online = $derived(data.peers.filter((p) => p.online).length);
	const rx = $derived(data.peers.reduce((a, p) => a + p.rx_bytes, 0));
	const tx = $derived(data.peers.reduce((a, p) => a + p.tx_bytes, 0));
	const short = (key: string) => `${key.slice(0, 8)}…`;
</script>

{#if !data.agent_installed}
	<EmptyState
		icon="ph-plugs"
		title="Quorum agent not installed"
		detail="WireGuard has no API, so a small agent in the WireGuard container reads the peers. Nothing answered at the base URL."
	/>
	<section class="card steps">
		<h2 class="section-title">Install the agent</h2>
		<ol>
			<li>
				Build it: <code class="mono">cd agent && cargo build --release</code>
			</li>
			<li>
				Copy <code class="mono">target/release/quorum-agent</code> and
				<code class="mono">agent/deploy/*</code> to the WireGuard container.
			</li>
			<li>
				Run <code class="mono">./install.sh ./quorum-agent &lt;Quorum server IP&gt;</code> as root. It
				prints the base URL and token.
			</li>
			<li>Put them in Settings → Services → WireGuard.</li>
		</ol>
	</section>
{:else}
	<div class="stats">
		<StatTile label="Peers online" value="{online} / {data.peers.length}" />
		<StatTile label="Received" value={bytes(rx)} />
		<StatTile label="Sent" value={bytes(tx)} />
		<StatTile
			label="Interfaces"
			value={data.interfaces.length}
			trend={data.interfaces
				.map((i) => `${i.name}${i.listen_port ? ` :${i.listen_port}` : ''}`)
				.join(', ')}
		/>
	</div>

	{#if data.peers.length === 0}
		<EmptyState icon="ph-plugs-connected" title="No peers configured" />
	{:else}
		<div class="card tablecard">
			<table class="table">
				<thead>
					<tr
						><th>Peer</th><th>Status</th><th>Last handshake</th><th class="hide-sm">Endpoint</th><th
							class="hide-sm">Allowed IPs</th
						><th class="right">Received</th><th class="right hide-sm">Sent</th></tr
					>
				</thead>
				<tbody>
					{#each data.peers as p (p.interface + p.public_key)}
						<tr>
							<td
								><span>{p.name ?? short(p.public_key)}</span><span class="caption faint mono block"
									>{p.name ? short(p.public_key) : ''}{data.interfaces.length > 1
										? ` · ${p.interface}`
										: ''}</span
								></td
							>
							<td
								><HealthDot
									health={p.online ? 'healthy' : 'disabled'}
									label={p.online ? 'Online' : 'Offline'}
								/></td
							>
							<td
								>{#if p.last_handshake}<RelativeTime value={p.last_handshake} />{:else}<span
										class="faint">Never</span
									>{/if}</td
							>
							<td class="mono muted hide-sm">{p.endpoint ?? '–'}</td>
							<td class="mono muted hide-sm">{p.allowed_ips.join(', ')}</td>
							<td class="right mono num">{bytes(p.rx_bytes)}</td>
							<td class="right mono num hide-sm">{bytes(p.tx_bytes)}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
{/if}

<style>
	.stats {
		display: grid;
		gap: 10px;
		grid-template-columns: repeat(4, minmax(0, 1fr));
	}
	.tablecard {
		padding: 4px 8px;
		overflow-x: auto;
	}
	.block {
		display: block;
	}
	.steps {
		padding: 16px 20px;
	}
	.section-title {
		margin-bottom: 8px;
	}
	ol {
		margin: 0;
		padding-left: 20px;
		display: flex;
		flex-direction: column;
		gap: 6px;
		font-size: 13px;
	}
	code {
		font-size: 12px;
		padding: 1px 5px;
		border-radius: 4px;
		background: var(--color-raised);
	}
	@media (max-width: 900px) {
		.stats {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
	}
</style>
