// Run a module action (`POST /services/{id}/actions/{action}`) with a toast that shows
// progress and then the result.
import { api, errorMessage, unwrap, type Schemas } from '#lib/api/client.js';
import { toasts } from '#lib/toasts.svelte.js';

export interface ActionOptions {
	/** Toast title while running, e.g. "Scanning Movies". */
	label: string;
	args?: Record<string, unknown>;
	/** Destructive actions need the caller to have asked already. */
	confirm?: boolean;
	/** Toast sub line. */
	sub?: string;
}

export async function runAction(
	serviceId: string,
	action: string,
	opts: ActionOptions
): Promise<Schemas['ActionResult'] | null> {
	const toast = toasts.push({ kind: 'progress', title: opts.label, sub: opts.sub });
	try {
		const result = unwrap(
			await api.POST('/services/{id}/actions/{action}', {
				params: { path: { id: serviceId, action } },
				body: { confirm: opts.confirm ?? false, args: (opts.args ?? {}) as never }
			})
		);
		toasts.update(toast, {
			kind: result.ok ? 'success' : 'error',
			title: opts.label,
			sub: result.message
		});
		return result;
	} catch (e) {
		toasts.update(toast, { kind: 'error', title: `${opts.label} failed`, sub: errorMessage(e) });
		return null;
	}
}

/** URL of a poster/cover proxied through the backend. */
export function imageUrl(serviceId: string, path: string | null | undefined): string | null {
	if (!path) return null;
	return `/api/v1/services/${encodeURIComponent(serviceId)}/image?path=${encodeURIComponent(path)}`;
}
