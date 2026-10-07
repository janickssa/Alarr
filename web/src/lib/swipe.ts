// Swipe actions for list rows on touch screens: drag horizontally past the threshold and
// release to run the action for that direction. The row follows the finger and is tinted
// with the action's colour; buttons on the row stay the accessible way to do the same.
import type { Action } from 'svelte/action';

export interface SwipeOptions {
	/** Swipe towards the left (reveals the right edge), e.g. delete or power. */
	left?: { label: string; color: string; run: () => void };
	/** Swipe towards the right, e.g. approve or start. */
	right?: { label: string; color: string; run: () => void };
	disabled?: boolean;
}

const THRESHOLD = 80;

export const swipe: Action<HTMLElement, SwipeOptions> = (node, initial) => {
	let opts = initial;
	let start: { x: number; y: number } | null = null;
	let dx = 0;
	let horizontal: boolean | null = null;

	function reset() {
		node.style.transition = 'transform 160ms ease, box-shadow 160ms ease';
		node.style.transform = '';
		node.style.boxShadow = '';
		node.removeAttribute('data-swipe');
		start = null;
		horizontal = null;
		dx = 0;
	}

	function onstart(e: TouchEvent) {
		if (opts.disabled || e.touches.length !== 1) return;
		// Leave the screen edge to the back gesture.
		if (e.touches[0].clientX < 24) return;
		start = { x: e.touches[0].clientX, y: e.touches[0].clientY };
		node.style.transition = 'none';
	}

	function onmove(e: TouchEvent) {
		if (!start) return;
		const t = e.touches[0];
		const mx = t.clientX - start.x;
		const my = t.clientY - start.y;
		if (horizontal === null && Math.abs(mx) + Math.abs(my) > 10) {
			horizontal = Math.abs(mx) > Math.abs(my) * 1.5;
		}
		if (!horizontal) return;
		const side = mx < 0 ? opts.left : opts.right;
		if (!side) return;
		e.preventDefault();
		dx = Math.max(-140, Math.min(140, mx));
		const past = Math.abs(dx) >= THRESHOLD;
		node.style.transform = `translateX(${dx}px)`;
		node.style.boxShadow = `inset ${dx < 0 ? '-' : ''}${Math.min(Math.abs(dx), 140)}px 0 0 -${past ? 0 : 40}px ${side.color}`;
		node.setAttribute('data-swipe', past ? side.label : '');
	}

	function onend() {
		if (!start) return;
		const side = dx < 0 ? opts.left : opts.right;
		const fire = horizontal && side && Math.abs(dx) >= THRESHOLD;
		reset();
		// Run after the snap-back: a transformed row would become the containing block for
		// fixed-position sheets opened by the action.
		if (fire) setTimeout(() => side.run(), 170);
	}

	node.addEventListener('touchstart', onstart, { passive: true });
	node.addEventListener('touchmove', onmove, { passive: false });
	node.addEventListener('touchend', onend);
	node.addEventListener('touchcancel', reset);
	return {
		update(next) {
			opts = next;
		},
		destroy() {
			node.removeEventListener('touchstart', onstart);
			node.removeEventListener('touchmove', onmove);
			node.removeEventListener('touchend', onend);
			node.removeEventListener('touchcancel', reset);
		}
	};
};
