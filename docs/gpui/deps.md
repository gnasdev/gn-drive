# Dependency inventory (checked 2026-10-22)

Legend: **MAJOR** = latest major differs from current. "Latest" = newest stable (no pre-releases).

## Toolchains
| Item | Declared | Installed locally | Source |
|---|---|---|---|
| swift-tools-version | 6.0 | Swift 6.4 (swift-driver 1.168.6) | Package.swift:1 |
| Platform min | macOS 14 (`.macOS(.v14)`) | – | Package.swift |
| Go | `go 1.26.5` (README: Go 1.26.5+) | go1.27.0 | go.mod, README.md:53 |
| Node / pnpm | no `engines`/.nvmrc; README: pnpm 11+ | Node v26.5.0 | README.md:53 |
| rclone | not version-pinned. Bundled into `GNDrive.app/Contents/MacOS/rclone` and the CLI tarball by copying whatever `command -v rclone` (or `RCLONE_SRC`) resolves to at packaging time; release CI does `brew install rclone` (scripts/bundle-app.sh:34-41, .github/workflows/release.yml:30-31) | v1.75.0 (/opt/homebrew/bin/rclone) | latest release tag **v1.75.2** (patch behind) |

## Swift (live)
| Dependency | Current (Package.resolved) | Requirement | Latest | Major? | Source |
|---|---|---|---|---|---|
| apple/swift-argument-parser | 1.8.2 (rev 6a52f3251125d74daf04fcbd5e6f08a75d074382) | `upToNextMajor(from: 1.5.0)` | 1.8.2 | no (up to date) | Package.swift, Package.resolved |
| CArgon2 (vendored, Sources/CArgon2) | no version recorded; headers say "Argon2 reference source code package - reference C implementations" (Sources/CArgon2/include/argon2.h:2); algorithm version enum 0x13 | – | n/a (not tracked) | – | vendored |
| sqlite3 (system lib) | system | `linkedLibrary("sqlite3")` | n/a | – | Package.swift |
| CryptoKit (system) | system | – | n/a | – | Sources |

Only one SwiftPM dependency exists.

## Go (legacy, reference only) — go.mod
Latest from `go list -m -u all` unless noted (proxy.golang.org).

| Module | Current | Latest | Major? | Direct |
|---|---|---|---|---|
| github.com/alexedwards/argon2id | v1.0.0 | v1.0.0 | no | yes |
| github.com/coder/websocket | v1.8.14 | v1.8.15 | no | yes |
| github.com/go-chi/chi/v5 | v5.3.1 | v5.5.0 | no | yes |
| github.com/gofrs/flock | v0.13.0 | v0.13.1 | no | yes |
| github.com/google/uuid | v1.6.0 | v1.6.0 | no | yes |
| github.com/robfig/cron/v3 | v3.0.1 | v3.0.1 | no | yes |
| github.com/spf13/cobra | v1.10.2 | v1.10.2 | no | yes |
| github.com/zalando/go-keyring | v0.2.8 | v0.2.8 | no | yes |
| golang.org/x/crypto | v0.54.0 | v0.58.0 | no | yes |
| modernc.org/sqlite | v1.56.0 | v1.61.0 | no | yes |
| github.com/danieljoos/wincred | v1.2.3 | v1.2.3 | no | indirect |
| github.com/dustin/go-humanize | v1.0.1 | v1.1.0 | no | indirect |
| github.com/godbus/dbus/v5 | v5.2.2 | v5.2.4 | no | indirect |
| github.com/inconshreveable/mousetrap | v1.1.0 | v1.1.0 | no | indirect |
| github.com/mattn/go-isatty | v0.0.24 | v0.0.24 | no | indirect |
| github.com/ncruces/go-strftime | v1.0.0 | v1.1.0 | no | indirect |
| github.com/remyoudompheng/bigfft | v0.0.0-20230129092748-24d4a6f8daec | same (no newer) | no | indirect |
| github.com/spf13/pflag | v1.0.10 | v1.0.10 | no | indirect |
| golang.org/x/sys | v0.47.0 | v0.49.0 | no | indirect |
| modernc.org/libc | v1.74.4 | v1.79.0 | no | indirect |
| modernc.org/mathutil | v1.7.1 | v1.7.1 | no | indirect |
| modernc.org/memory | v1.11.0 | v1.12.1 | no | indirect |

No Go module has a major bump. (Graph-only modules like x/net, x/text, testify also have updates but are not declared in go.mod.)

## Frontend (legacy Vue SPA, reference only) — frontend/package.json, pnpm
"Current" = pnpm-lock.yaml version for dev deps; for runtime deps `npm outdated` "wanted" (resolved from declared range; node_modules absent so current blank; lock has e.g. vue 3.5.40). "Latest" from npm registry.

| Package | Range | Current | Latest | Major? | Kind |
|---|---|---|---|---|---|
| @fontsource-variable/geist | ^5.3.0 | 5.3.0 | 5.3.0 | no | dep |
| @phosphor-icons/vue | ^2.2.1 | 2.2.1 | 2.2.1 | no | dep |
| @tanstack/vue-virtual | ^3.13.35 | 3.13.47 | 3.13.47 | no | dep |
| @vue-flow/background | ^1.3.2 | 1.3.2 | 1.3.2 | no | dep |
| @vue-flow/controls | ^1.1.3 | 1.1.3 | 1.1.3 | no | dep |
| @vue-flow/core | ^1.48.2 | 1.48.2 | 1.48.2 | no | dep |
| clsx | ^2.1.0 | 2.1.1 | 2.1.1 | no | dep |
| pinia | ^3.0.4 | 3.0.4 | 4.1.0 | **MAJOR** | dep |
| tailwind-merge | ^3.0.0 | 3.7.0 | 3.7.0 | no | dep |
| vue | ^3.5.40 | 3.5.40 (lock) | 3.5.44 | no | dep |
| vue-i18n | ^11.4.8 | 11.4.16 | 11.4.16 | no | dep |
| vue-router | ^4.6.4 | 4.6.4 | 5.4.0 | **MAJOR** | dep |
| @tailwindcss/vite | ^4.3.3 | 4.3.3 | 4.3.3 | no | dev |
| @types/node | ^26.1.2 | 26.1.2 | 26.6.5 | no | dev |
| @vitejs/plugin-vue | ^6.0.8 | 6.0.8 | 6.0.10 | no | dev |
| nyc | ^18.0.0 | 18.0.0 | 18.0.0 | no | dev |
| puppeteer | ^25.5.0 | 25.5.0 | 25.15.0 | no | dev |
| tailwindcss | ^4.3.3 | 4.3.3 | 4.3.3 | no | dev |
| tsx | ^4.23.5 | 4.23.5 | 4.23.15 | no | dev |
| typescript | ^5.9.3 | 5.9.3 | 7.0.2 | **MAJOR** | dev |
| vite | ^7.3.0 | 7.3.6 | 8.3.5 | **MAJOR** | dev |
| vite-plugin-istanbul | ^9.0.1 | 9.0.1 | 9.0.1 | no | dev |
| vite-plugin-pwa | ^1.2.0 | 1.3.0 | 2.0.0 | **MAJOR** | dev |
| vue-tsc | ^3.3.9 | 3.3.9 | 3.3.13 | no | dev |

## Notes
- Live: Swift package (GNDriveCore, gn-drive CLI, GNDriveApp). Only external SwiftPM dep is swift-argument-parser (already latest). Vendored CArgon2 must keep `$argon2id$v=19$m=65536,t=3,p=4$…` compatibility (AGENTS.md:5).
- Legacy (reference only): Go backend (go.mod) and Vue SPA (frontend/). No action needed to upgrade them.
- rclone is shipped inside the app bundle but its version is whatever Homebrew provides at release time (unpinned); the local install is v1.75.0, latest upstream tag v1.75.2. The GPUI window should reach rclone only through the engine (`gn-drive serve`), never spawn it itself.
- `google/uuid`, `robfig/cron`, `go-keyring` etc. latest from proxy.golang.org; robfig/cron's non-/v3 path reports v1.2.0 but the declared /v3 module's latest is v3.0.1.
- Rust GPUI port needs replacements for: argument parsing (clap), Argon2id, AES-256-GCM, SQLite, keyring — none of the above versions constrain this.

## Final upgrade result (2026-10-22)

The tables above are the pre-upgrade inventory. Applied from → to:

### Swift
| Dependency | From | To |
|---|---|---|
| swift-argument-parser | 1.8.2 | 1.8.2 (already latest; no change) |

### Go (legacy; `go test ./... -count=1` passes with no rclone on PATH)
| Item | From | To |
|---|---|---|
| go directive / .go-version | 1.26.5 | 1.27.0 |
| github.com/coder/websocket | v1.8.14 | v1.8.15 |
| github.com/go-chi/chi/v5 | v5.3.1 | v5.5.0 |
| github.com/gofrs/flock | v0.13.0 | v0.13.1 |
| golang.org/x/crypto | v0.54.0 | v0.58.0 |
| modernc.org/sqlite | v1.56.0 | v1.61.0 |
| (indirect) go-humanize / dbus / go-strftime | v1.0.1 / v5.2.2 / v1.0.0 | v1.1.0 / v5.2.4 / v1.1.0 |
| (indirect) x/sys / modernc libc / memory | v0.47.0 / v1.74.4 / v1.11.0 | v0.49.0 / v1.79.0 / v1.12.1 |

### Frontend (legacy; `pnpm run build`, `type-check`, `test:unit` 33/33 pass)
| Package | From | To |
|---|---|---|
| @tanstack/vue-virtual | ^3.13.35 | ^3.13.47 |
| clsx | ^2.1.0 | ^2.1.1 |
| pinia | ^3.0.4 | ^4.1.0 (**major**) |
| tailwind-merge | ^3.0.0 | ^3.7.0 |
| vue | ^3.5.40 | ^3.5.44 |
| vue-i18n | ^11.4.8 | ^11.4.16 |
| vue-router | ^4.6.4 | ^5.4.0 (**major**) |
| @types/node | ^26.1.2 | ^26.6.5 |
| @vitejs/plugin-vue | ^6.0.8 | ^6.0.10 |
| puppeteer | ^25.5.0 | ^25.15.0 |
| tsx | ^4.23.5 | ^4.23.15 |
| typescript | ^5.9.3 | ^6.0.3 (**held below latest 7.0.2**: vue-tsc 3.3.13 requires `typescript/lib/tsc`, which TS 7 does not export) |
| vite | ^7.3.0 | ^8.3.5 (**major**) |
| vite-plugin-pwa | ^1.2.0 | ^2.0.0 (**major**) |
| vue-tsc | ^3.3.9 | ^3.3.13 |

e2e (`test:e2e`, puppeteer) was not run: it launches a browser against a real backend and the legacy UI is reference-only.
