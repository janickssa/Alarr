// Renders the PWA icons and iOS splash screens from the design (batch 1, board 1j) with
// Chromium. Output goes to static/icons and static/splash and is committed.
//
//   NODE_PATH=$(npm root -g) node scripts/pwa-assets.mjs     (needs `playwright` installed)
import { mkdirSync, writeFileSync, readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const { chromium } = require('playwright');
const root = fileURLToPath(new URL('..', import.meta.url));

/** The mark from the design, with its fill matched to the background behind it. */
const mark = (fill) => `<svg viewBox="0 0 48 48" fill="none" xmlns="http://www.w3.org/2000/svg">
<path d="M24 10 L11 34 L37 34 Z" stroke="#b5abfc" stroke-width="2.2" stroke-linejoin="round" opacity=".6"/>
<circle cx="24" cy="10" r="5" fill="${fill}" stroke="#e7e5fe" stroke-width="2.4"/>
<circle cx="11" cy="34" r="5" fill="${fill}" stroke="#e7e5fe" stroke-width="2.4"/>
<circle cx="37" cy="34" r="5" fill="${fill}" stroke="#e7e5fe" stroke-width="2.4"/>
<circle cx="24" cy="26" r="3.4" fill="#b5abfc"/></svg>`;

const TILE = 'radial-gradient(120% 90% at 30% 15%,#353b80 0%,#262a60 40%,#161826 100%)';

/** Icon tile: the mark fills 46 % (inside the 80 % maskable safe zone). */
function iconHtml(size, radius) {
	const m = Math.round(size * 0.46);
	return `<html><body style="margin:0;background:transparent">
<div style="width:${size}px;height:${size}px;border-radius:${radius}px;background:${TILE};
box-shadow:inset 0 0 0 ${Math.max(1, size / 120)}px rgba(181,171,252,.18);display:grid;place-items:center">
<div style="width:${m}px;height:${m}px;filter:drop-shadow(0 0 ${Math.max(1, size / 20)}px rgba(181,171,252,.45))">${mark('#262a60')}</div>
</div></body></html>`;
}

const inter = readFileSync(
	require.resolve('@fontsource/inter/files/inter-latin-500-normal.woff2', { paths: [root] })
).toString('base64');

/** Splash: as the design's 390 × 844 splash, scaled to the screen. */
function splashHtml(w, h, dpr) {
	const m = 56 * dpr;
	return `<html><head><style>@font-face{font-family:Inter;font-weight:500;src:url(data:font/woff2;base64,${inter}) format('woff2')}</style></head>
<body style="margin:0"><div style="width:${w}px;height:${h}px;background:radial-gradient(60% 40% at 50% 44%,#262a60 0%,#161826 70%);
display:flex;flex-direction:column;align-items:center;justify-content:center;gap:${14 * dpr}px">
<div style="width:${m}px;height:${m}px;filter:drop-shadow(0 0 ${10 * dpr}px rgba(181,171,252,.5))">${mark('#1e2142')}</div>
<span style="font:500 ${17 * dpr}px Inter;letter-spacing:-0.015em;color:#e9e9ed">Quorum</span>
</div></body></html>`;
}

// Portrait launch screens: [css width, css height, device pixel ratio].
export const SPLASHES = [
	[440, 956, 3],
	[430, 932, 3],
	[428, 926, 3],
	[414, 896, 3],
	[414, 896, 2],
	[402, 874, 3],
	[393, 852, 3],
	[390, 844, 3],
	[375, 812, 3],
	[375, 667, 2],
	[1024, 1366, 2],
	[834, 1194, 2],
	[820, 1180, 2],
	[810, 1080, 2]
];

const browser = await chromium.launch();
const page = await browser.newPage();
async function shot(html, w, h, file, transparent = false) {
	await page.setViewportSize({ width: w, height: h });
	await page.setContent(html);
	await page.evaluate(() => document.fonts.ready);
	// Splash screens are large gradients: JPEG keeps them ~10× smaller than PNG.
	const jpeg = file.endsWith('.jpg');
	await page.screenshot({
		path: file,
		omitBackground: transparent,
		type: jpeg ? 'jpeg' : 'png',
		...(jpeg ? { quality: 82 } : {})
	});
}

mkdirSync(`${root}/static/icons`, { recursive: true });
mkdirSync(`${root}/static/splash`, { recursive: true });
// "any" icons keep rounded corners (Android shows them as they are); maskable, Apple and
// favicon sources are full-bleed (the OS applies its own shape).
await shot(iconHtml(512, 115), 512, 512, `${root}/static/icons/icon-512.png`, true);
await shot(iconHtml(192, 43), 192, 192, `${root}/static/icons/icon-192.png`, true);
await shot(iconHtml(512, 0), 512, 512, `${root}/static/icons/maskable-512.png`);
await shot(iconHtml(192, 0), 192, 192, `${root}/static/icons/maskable-192.png`);
await shot(iconHtml(180, 0), 180, 180, `${root}/static/icons/apple-touch-icon.png`);
await shot(iconHtml(32, 7), 32, 32, `${root}/static/icons/favicon-32.png`, true);
await shot(iconHtml(16, 3), 16, 16, `${root}/static/icons/favicon-16.png`, true);
writeFileSync(
	`${root}/static/icons/favicon.svg`,
	`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><defs><radialGradient id="g" cx="30%" cy="15%" r="120%"><stop offset="0" stop-color="#353b80"/><stop offset=".4" stop-color="#262a60"/><stop offset="1" stop-color="#161826"/></radialGradient></defs><rect width="48" height="48" rx="10" fill="url(#g)"/><g transform="translate(12.96 12.96) scale(.46)">${mark('#262a60').replace(/^<svg[^>]*>|<\/svg>$/g, '')}</g></svg>\n`
);
for (const [w, h, dpr] of SPLASHES) {
	await shot(
		splashHtml(w * dpr, h * dpr, dpr),
		w * dpr,
		h * dpr,
		`${root}/static/splash/${w * dpr}x${h * dpr}.jpg`
	);
}
await browser.close();
console.log('done');
