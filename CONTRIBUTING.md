# Contributing

The [README](README.md) is the single source of scope, phases, architecture and the decisions log. Read it first and follow its "Instructions for the coding LLM", which apply to human contributors too:

- Work phase by phase; tick the README checkboxes in the commit that completes each task.
- Small commits with clear, scoped messages (`backend: …`, `web: …`, `ci: …`).
- `design/` is the design export as delivered. Do not edit it.
- Never commit secrets, real IPs or hostnames.
- Record ambiguous choices in the README's Decisions log.

## Running locally

```sh
cd backend && cargo run -- --demo        # fake data, sign in as demo/demo
cd web && npm run dev                    # web dev server
```

## Local checks (same as CI)

Backend (`backend/`):

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Web (`web/`):

```sh
npm ci
npm run lint
npm run check
npm run build
```

OpenAPI: after changing any API type or route, regenerate and commit the spec:

```sh
cd backend && cargo run -- openapi > ../openapi/openapi.json
```
