import { dev } from '$app/env';
import { error } from '@sveltejs/kit';

// The component gallery only exists in dev builds.
export function load() {
	if (!dev) error(404, 'Not found');
}
