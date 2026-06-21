# Security policy

Amdion runs entirely on your Mac: a local Tauri app, a local SQLite store, a
loopback-only WebSocket bridge to a companion Chrome extension, and a
GitHub-Releases auto-updater. The V1 spine sends nothing off-device.

## Reporting a vulnerability

Please report security issues **privately** — do not open a public issue or PR.

- Use GitHub's **private vulnerability reporting** for this repository
  (the *Security* tab → *Report a vulnerability*). Enable it under
  *Settings → Code security → Private vulnerability reporting* if it isn't on.
- Include a description, affected version/commit, and steps to reproduce.

We aim to acknowledge reports within a few days and will coordinate a fix and
disclosure timeline with you.

## Known limitations (V1)

These are deliberate, documented trade-offs in the build-from-source V1, not
unreported bugs:

- **Loopback bridge trust model.** The app↔extension bridge listens on
  `127.0.0.1` and authenticates the connecting party by a pinned extension
  `Origin`. This stops any web page from connecting (browser JavaScript cannot
  forge the `Origin` header). However, a session token is generated but **not
  yet enforced** (`REQUIRE_TOKEN = false`), so any *other local process* that
  presents the pinned `Origin` can issue bridge commands and read pushed config
  (the block list and current intent). It cannot read your activity/notes
  stream, which flows extension→app only. Token enforcement is planned for the
  packaged / Chrome Web Store build.
- **Auto-updater.** Updates are fetched over HTTPS from GitHub Releases and
  verified against a pinned minisign public key before installation. The
  private signing key is held outside the repository.

## Scope

In scope: the desktop app, the bridge protocol, the extension, and the updater.
Out of scope: issues that require an already-compromised local user account, or
the optional, off-by-default assistant integration (disabled in V1).
