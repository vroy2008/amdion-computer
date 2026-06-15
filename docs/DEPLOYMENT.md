# Amdion — Deployment Plan

How Amdion gets from this repo onto other people's Macs, kept current, and
downloadable from amdion.org.

## Two truths to design around

1. **Amdion is a macOS app, not cross-platform.** It's built on Mac-only APIs
   (`NSWorkspace` frontmost polling, a CoreGraphics idle FFI, the menu-bar /
   Accessory paradigm, a LaunchAgent for login-start) and the product itself is
   deliberately Mac-tuned. Windows/Linux would be a real port, not a recompile.
   So the site says **"Download for Mac"**, and non-Mac visitors get an email
   capture, not a broken download.

2. **A build must be signed + notarized or it won't open.** Without an Apple
   Developer ID signature and Apple notarization, macOS Gatekeeper blocks the
   download on every Mac but the one that built it ("Apple could not verify…" /
   "is damaged and can't be opened"). This is the single gate between "I have a
   DMG" and "anyone can install it."

## Distribution channel

**Direct distribution** (Developer ID + notarization), downloaded from
amdion.org, hosted on GitHub Releases, kept current by an in-app auto-updater.

We **skip the Mac App Store**: its sandbox would break the global shortcut,
`NSWorkspace` polling, CoreGraphics idle FFI, LaunchAgent autostart, and the
localhost WebSocket bridge to Chrome. Direct distribution is the correct channel
for an app like this.

---

## Status checklist

### ✅ Done in the repo (no Apple account needed)

- [x] **Updater signing keypair generated** — minisign keypair at
      `~/.amdion/amdion-updater.key` (private, gitignored by living outside the
      repo) + `.pub`. Public key embedded in `tauri.conf.json`.
- [x] **In-app auto-updater wired** — `tauri-plugin-updater` added; checks
      GitHub Releases ~10s after launch and stages any newer *signed* build
      (applied next launch). Manual `check_for_updates` command also exposed for
      a future Settings button. See `src-tauri/src/commands/updater.rs`.
- [x] **Universal build + signing scaffolding** — `tauri.conf.json` gains
      `createUpdaterArtifacts`, `bundle.macOS` (entitlements + min OS), and the
      updater `pubkey`/`endpoints`. `entitlements.plist` added for the hardened
      runtime. Signing is env-var-driven, so it "just works" once the cert
      exists — no code change needed later.
- [x] **CI release workflow** — `.github/workflows/release.yml` builds a
      universal DMG, signs, notarizes, signs the updater artifact, and drafts a
      GitHub Release with `latest.json`. Runs on `git push` of a `v*` tag.

### 🔒 Needs the Apple Developer account (the gate — start this in parallel)

- [ ] **Enrol in the Apple Developer Program** — $99/yr, ~1–2 day approval.
      <https://developer.apple.com/programs/>
- [ ] **Create a Developer ID Application certificate** (Xcode or the developer
      portal), export it as a password-protected `.p12`.
- [ ] **Create an app-specific password** for your Apple ID (for notarization):
      <https://account.apple.com> → Sign-In and Security → App-Specific Passwords.
- [ ] **Add the GitHub Actions secrets** listed below.

### 🚀 First release (once the secrets exist)

- [ ] `git tag v1.0.1 && git push origin v1.0.1` → CI builds/signs/notarizes →
      a **draft** Release appears.
- [ ] Download the DMG from the draft on a *different* Mac (or a fresh user) and
      confirm it opens with no Gatekeeper warning.
- [ ] **Publish** the draft Release. (Publishing is what makes the updater and
      the website download point at it — `/releases/latest/...`.)

### 🌐 Website (amdion.org)

- [ ] Add a **"Download for Mac"** button → latest DMG (see snippet below).
- [ ] Add a simple **email capture** for release news / non-Mac visitors.

### 🍺 Optional, later

- [ ] **Homebrew cask** (`brew install --cask amdion`) for technical users.

---

## GitHub Actions secrets

Settings → Secrets and variables → Actions → New repository secret:

| Secret | What it is |
| --- | --- |
| `APPLE_CERTIFICATE` | base64 of the Developer ID Application `.p12` (`base64 -i cert.p12 \| pbcopy`) |
| `APPLE_CERTIFICATE_PASSWORD` | the `.p12` export password |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_ID` | your Apple Developer account email |
| `APPLE_PASSWORD` | an **app-specific** password for that Apple ID |
| `APPLE_TEAM_ID` | your 10-character Team ID |
| `TAURI_SIGNING_PRIVATE_KEY` | contents of `~/.amdion/amdion-updater.key` (`cat ~/.amdion/amdion-updater.key \| pbcopy`) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | empty string `""` (the key was generated without a password) |

> ⚠️ **Back up `~/.amdion/amdion-updater.key` somewhere safe (a password
> manager).** If it's lost, you can't sign updates that existing installs will
> accept — every user would have to manually re-download. The `.pub` half is
> already committed in `tauri.conf.json`; the two must stay a matched pair.

---

## How the auto-update flow works

1. CI builds a release and publishes `AMDION.app.tar.gz`, its `.sig`, and
   `latest.json` to a GitHub Release.
2. A running Amdion checks
   `https://github.com/vroy2008/amdion-computer/releases/latest/download/latest.json`
   ~10s after launch.
3. If `latest.json`'s version is newer, the app downloads the `.tar.gz`, verifies
   its `.sig` against the `pubkey` baked into the build, and stages it.
4. The update applies on the user's next launch. No re-download, no prompt.

This is why "sign up for updates" by email is secondary: the app keeps itself
current. Keep an email list for **announcements** (and for non-Mac visitors),
not as the update mechanism.

---

## Website: "Download for Mac" snippet

Drop-in starting point — adapt to amdion.org's stack. The `latest/download`
path always resolves to the newest published release, so the link never needs
updating per-release.

```html
<a class="amdion-download"
   href="https://github.com/vroy2008/amdion-computer/releases/latest/download/AMDION_universal.dmg">
  Download for Mac
</a>
<p class="amdion-req">Free · macOS 10.15+ · Intel &amp; Apple Silicon</p>
```

> Note: the exact DMG asset filename includes the version
> (`AMDION_1.0.1_universal.dmg`). Either (a) link to the Releases page and let
> users pick, (b) add a tiny redirect on amdion.org that points at the current
> asset, or (c) keep a stable-named copy. Simplest at first: link the **latest
> release page** (`/releases/latest`) and let GitHub show the DMG.

For non-Mac visitors, swap the button for an email field ("Amdion is Mac-only
today — get notified if we add your platform").

---

## Local build (for testing before tagging)

```bash
# One-time: add the Intel target so universal builds work locally.
rustup target add x86_64-apple-darwin

# Unsigned local universal build (won't pass Gatekeeper elsewhere, fine for you).
npm install
npx tauri build --target universal-apple-darwin

# To test signing/notarization locally, export the same env vars the CI uses
# (APPLE_* and TAURI_SIGNING_PRIVATE_KEY) before `tauri build`.
```

Artifacts land in `src-tauri/target/universal-apple-darwin/release/bundle/`.

---

## Cost summary

| Item | Cost |
| --- | --- |
| Apple Developer Program | $99 / year |
| GitHub Releases hosting + bandwidth | $0 |
| GitHub Actions macOS minutes (public repo) | $0 |
| Updater infrastructure | $0 (rides on Releases) |
| Email capture (Buttondown free tier / similar) | $0 to start |

The only required spend is the $99/yr Apple membership.
