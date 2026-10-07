// Health states: label, Phosphor icon and colours. Health is never shown by colour alone.
import type { Health } from '#lib/api/client.js';

export interface HealthMeta {
	label: string;
	icon: string;
	fg: string;
	fill: string;
}

export const HEALTH: Record<Health, HealthMeta> = {
	healthy: {
		label: 'Healthy',
		icon: 'ph-check-circle',
		fg: 'var(--health-healthy)',
		fill: 'var(--health-healthy-fill)'
	},
	warning: {
		label: 'Warning',
		icon: 'ph-warning',
		fg: 'var(--health-warning)',
		fill: 'var(--health-warning-fill)'
	},
	down: {
		label: 'Down',
		icon: 'ph-x-circle',
		fg: 'var(--health-down)',
		fill: 'var(--health-down-fill)'
	},
	in_progress: {
		label: 'In progress',
		icon: 'ph-circle-notch',
		fg: 'var(--health-progress)',
		fill: 'var(--health-progress-fill)'
	},
	disabled: {
		label: 'Disabled',
		icon: 'ph-minus-circle',
		fg: 'var(--health-neutral)',
		fill: 'var(--health-neutral-fill)'
	},
	unknown: {
		label: 'Unknown',
		icon: 'ph-question',
		fg: 'var(--health-neutral)',
		fill: 'var(--health-neutral-fill)'
	}
};

/** Colour for a usage fraction: neutral below 80 %, amber 80–89 %, red from 90 %. */
export function usageLevel(fraction: number | null | undefined): 'ok' | 'warning' | 'down' {
	if (fraction == null) return 'ok';
	if (fraction >= 0.9) return 'down';
	if (fraction >= 0.8) return 'warning';
	return 'ok';
}

export const CATEGORY_LABEL: Record<string, string> = {
	infrastructure: 'Infrastructure',
	media: 'Media',
	network_security: 'Network and security',
	apps: 'Apps'
};

export const CATEGORY_ORDER = ['infrastructure', 'media', 'network_security', 'apps'];
