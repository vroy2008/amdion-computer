# Third-party notices

Amdion is released under the [MIT License](LICENSE). It bundles and depends on
third-party components whose licenses are reproduced or referenced below.

## Bundled source

### Mozilla Readability — Apache License 2.0

`extension/features/read-mode/vendor/readability.js` and
`extension/features/read-mode/vendor/readability-readerable.js` are vendored,
unmodified, from Mozilla's Readability (originally Arc90 Inc.). Read Mode uses
them to extract the readable article body from a page.

- Upstream: https://github.com/mozilla/readability
- License: Apache License 2.0 — full text in
  [`extension/features/read-mode/vendor/LICENSE-readability.txt`](extension/features/read-mode/vendor/LICENSE-readability.txt)
- The original copyright and license headers are preserved in each file.

## Runtime dependencies (not redistributed in this repository)

- **Tauri** and its plugins — MIT / Apache-2.0 (https://tauri.app).
- Rust crates resolved via [`src-tauri/Cargo.lock`](src-tauri/Cargo.lock) — all
  permissive (MIT / Apache-2.0 / MPL-2.0). No GPL/AGPL/LGPL code is compiled
  into the macOS build.
- **Inter** and **Cinzel** fonts — SIL Open Font License 1.1, loaded at runtime
  from Google Fonts. No font files are redistributed in this repository.

## Trademark & brand

The name **AMDION**, the amdion.org identity, and the Amdion logo and icons are
brand assets of the author. The MIT license covers the source code only — it
does not grant any rights to the AMDION name or marks. Forks and derivatives
should use their own branding.
