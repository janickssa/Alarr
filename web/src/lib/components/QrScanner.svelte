<script lang="ts">
	// Camera QR scanner. Uses the browser's BarcodeDetector where available (Android Chrome)
	// and the jsQR decoder elsewhere (iOS Safari), loaded only when the camera opens.
	import { onDestroy, onMount } from 'svelte';

	let { ondetect, onerror }: { ondetect: (text: string) => void; onerror: (msg: string) => void } =
		$props();

	let video: HTMLVideoElement | undefined = $state();
	let stream: MediaStream | null = null;
	let raf = 0;
	let stopped = false;

	type Detector = { detect: (src: CanvasImageSource) => Promise<{ rawValue: string }[]> };

	onMount(async () => {
		if (!navigator.mediaDevices?.getUserMedia) {
			onerror('This browser has no camera access. Type the code instead.');
			return;
		}
		try {
			stream = await navigator.mediaDevices.getUserMedia({
				video: { facingMode: 'environment' },
				audio: false
			});
		} catch {
			onerror('Camera permission was denied. Type the code instead.');
			return;
		}
		if (!video || stopped) return stop();
		video.srcObject = stream;
		await video.play().catch(() => {});

		const Native = (globalThis as unknown as { BarcodeDetector?: new (o: object) => Detector })
			.BarcodeDetector;
		const native = Native ? new Native({ formats: ['qr_code'] }) : null;
		const jsQR = native ? null : (await import('jsqr')).default;
		const canvas = document.createElement('canvas');
		const ctx = canvas.getContext('2d', { willReadFrequently: true });

		const tick = async () => {
			if (stopped || !video) return;
			if (video.readyState >= 2) {
				let text: string | null = null;
				if (native) {
					const found = await native.detect(video).catch(() => []);
					text = found[0]?.rawValue ?? null;
				} else if (jsQR && ctx) {
					canvas.width = video.videoWidth;
					canvas.height = video.videoHeight;
					ctx.drawImage(video, 0, 0);
					const img = ctx.getImageData(0, 0, canvas.width, canvas.height);
					text = jsQR(img.data, img.width, img.height)?.data ?? null;
				}
				if (text) {
					stop();
					ondetect(text);
					return;
				}
			}
			raf = requestAnimationFrame(() => void tick());
		};
		void tick();
	});

	function stop() {
		stopped = true;
		cancelAnimationFrame(raf);
		stream?.getTracks().forEach((t) => t.stop());
		stream = null;
	}

	onDestroy(stop);
</script>

<div class="scanner">
	<video bind:this={video} playsinline muted aria-label="Camera preview"></video>
	<div class="frame" aria-hidden="true"></div>
</div>

<style>
	.scanner {
		position: relative;
		width: 100%;
		aspect-ratio: 1;
		border-radius: var(--radius-md);
		overflow: hidden;
		background: #000;
	}
	video {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}
	.frame {
		position: absolute;
		inset: 18%;
		border: 2px solid var(--color-accent-300);
		border-radius: 12px;
		box-shadow: 0 0 0 999px rgb(0 0 0 / 0.35);
	}
</style>
