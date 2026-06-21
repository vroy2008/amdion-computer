# Contributing to Amdion

Thanks for your interest! Amdion is an early, build-from-source project
(**macOS + Chrome**). This guide takes you from a clone to a running dev build.

## Prerequisites

- [Rust](https://rustup.rs) (via rustup)
- [Node.js](https://nodejs.org) 18+
- Google Chrome
- macOS 10.15+

## Build & run

```bash
npm install
npm run dev          # builds + launches the Tauri app (first build is slow)
```

Then load the extension: open `chrome://extensions`, enable **Developer mode**,
click **Load unpacked**, and choose this repo's `extension/` folder. The
popup shows **Connected** once it links to the app over the loopback bridge.

See the README "Quick start" for the full flow and
[docs/DEV.md](docs/DEV.md) for the dev loop, port ranges, and reset scripts
(`npm run dev:reset`, `dev:reset:hard`).

## Tests

```bash
cd src-tauri && cargo test --lib
cargo check          # fast type/borrow check
```

## Project layout & conventions

- **`src-tauri/`** — Tauri v2 Rust backend (sensing, SQLite, the bridge, commands).
- **`frontend/`** — the vanilla HTML/CSS/JS front door + Observer UI.
- **`extension/`** — Chrome MV3 extension, split into `core/` (the V1 spine:
  bridge, activity tracking, nudge, block) and `features/` (the "bonus shelf").
  **Keep that boundary** — V1 changes belong in `core/`.
- **Scope of truth** for what is in V1 is [docs/V1.md](docs/V1.md).
- **Commit messages** follow Conventional Commits with a scope, e.g.
  `feat(modes): ...`, `refactor(panel): ...`, `docs(readme): ...`.
- Keep pull requests focused and describe what you verified.

## Reporting

- **Bugs / ideas:** open a GitHub issue.
- **Security:** see [SECURITY.md](SECURITY.md) — please report privately.

## License

By contributing, you agree that your contributions are licensed under the
project's [MIT License](LICENSE).
