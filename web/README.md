# Quorum web

SvelteKit single-page app (Svelte 5, TypeScript, `adapter-static`). The production build in `build/` is embedded into `quorum-server`, which serves it with an SPA fallback.

See the [project README](../README.md) for scope, phases and the design rules.

```sh
npm ci
npm run dev      # dev server; proxies /api to QUORUM_DEV_BACKEND (default http://127.0.0.1:8080)
npm run gen:api  # regenerate src/lib/api/schema.d.ts from ../openapi/openapi.json
npm run lint     # prettier + eslint
npm run check    # svelte-check / TypeScript
npm run build    # static build into build/
```

Run the backend with fake data for development: `cd ../backend && cargo run -- --demo` (sign in as `demo` / `demo`).

## Layout

| Path | What |
| --- | --- |
| `src/lib/styles/` | Design tokens (`tokens.css`, from the Nocturne design system + batch 1) and base/component classes |
| `src/lib/api/` | Generated API types and the typed client |
| `src/lib/*.svelte.ts` | Shared state: session, live updates (SSE), connection, toasts, power actions |
| `src/lib/components/` | Component library matching design batch 1 |
| `src/routes/` | Pages. `/dev/components` is the component gallery (dev server only) |
