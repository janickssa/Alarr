<script lang="ts">
	// Phosphor icon (self-hosted font). Decorative unless `label` is given.
	let {
		name,
		fill = false,
		size,
		color,
		label,
		spin = false,
		class: cls = ''
	}: {
		name: string;
		fill?: boolean;
		size?: number;
		color?: string;
		label?: string;
		spin?: boolean;
		class?: string;
	} = $props();
	const glyph = $derived(name.startsWith('ph-') ? name : `ph-${name}`);
</script>

<i
	class="{fill ? 'ph-fill' : 'ph'} {glyph} {cls}"
	class:spin
	style:font-size={size ? `${size}px` : undefined}
	style:color
	aria-hidden={label ? undefined : 'true'}
	role={label ? 'img' : undefined}
	aria-label={label}
	title={label}
></i>
