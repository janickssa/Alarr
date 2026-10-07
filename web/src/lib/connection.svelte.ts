// Backend reachability and sign-in state, shared by the API client and the SSE stream.

class Connection {
	/** False after a request failed at the network level (not on VPN, server down). */
	reachable = $state(true);
	/** Last time any response or event arrived. */
	lastSeen = $state<Date | null>(null);
	/** Set when an authenticated request got 401. The layout redirects to sign-in. */
	signedOut = $state(false);
	/** SSE stream state. */
	live = $state<'connecting' | 'open' | 'closed'>('connecting');

	markReachable() {
		this.reachable = true;
		this.lastSeen = new Date();
	}

	markUnreachable() {
		this.reachable = false;
	}

	markSignedOut() {
		this.signedOut = true;
	}
}

export const connection = new Connection();
