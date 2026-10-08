// Phone navigation: which pages are "pushed" (show a back button and accept the edge swipe)
// and where back goes.
import { goto } from '$app/navigation';
import { resolve } from '$app/paths';

const MORE = ['/health', '/alerts', '/activity', '/launcher', '/settings'];

/** Where back leads from a pushed page, or `null` for a top-level destination. */
export function parentOf(path: string): string | null {
	if (/^\/services\/.+/.test(path)) return resolve('/services');
	if (/^\/settings\/.+/.test(path)) return resolve('/settings');
	if (MORE.some((m) => path === m)) return resolve('/more');
	return null;
}

class Nav {
	/** Number of in-app navigations; history.back() is safe when > 0. */
	depth = $state(0);

	back(path: string) {
		if (this.depth > 0) history.back();
		else {
			const parent = parentOf(path);
			if (parent) void goto(parent, { replaceState: true });
		}
	}
}

export const nav = new Nav();
