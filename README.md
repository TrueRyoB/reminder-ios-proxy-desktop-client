# reminder-proxy-client

An unofficial Windows client/daemon for iCloud Reminders, built to work
around the poor UX of [iCloud.com Reminders for Web](https://icloud.com/reminders)
on Windows: no real-time notifications, no manual reordering, and no quick
priority/flag/list editing.

It talks directly to the same private CloudKit web API that icloud.com's own
Reminders page uses — there is no official public API for Reminders, and (as
of iOS 13 / macOS Catalina) CalDAV no longer carries the modern Reminders
format at all.

## ⚠️ Disclaimer

This project uses an undocumented, private Apple API by replaying the same
requests icloud.com's own web frontend makes, authenticated with your own
Apple ID session. It is not affiliated with or endorsed by Apple. Apple can
change this API at any time without notice, which may break this tool.
Use at your own risk, on your own account.

## Download (desktop app)

The desktop app — **reminder** — is the primary way to use this project. Get the
latest Windows installer from the
[Releases page](https://github.com/TrueRyoB/reminder-ios-proxy-desktop-client/releases):

| File | Notes |
|---|---|
| `reminder_<version>_x64-setup.exe` | NSIS, per-user install. **Recommended.** |
| `reminder_<version>_x64_en-US.msi` | MSI alternative; normally unnecessary. |

The installer is **not code-signed**, so Windows SmartScreen will warn on first
run — choose *More info* → *Run anyway*.

Requirements: Windows 10/11 x64, and an Apple ID with two-factor
authentication. Notifications only fire while the app is running (it lives in
the system tray; closing the window hides it rather than quitting), because
Apple exposes no push mechanism for Reminders.

The CLI below (`reminder-proxy-client`) is a debugging/verification tool for the
same core library, not the intended end-user surface.

## Features

- **Real-time-ish notifications**: background polling + Windows toast
  notifications for due reminders (no Apple push exists for Reminders, so
  this requires the process to be running).
- **Manual reordering**: rewrites the list's `ReminderIDs` field directly —
  not exposed by any other known third-party client.
- **Quick edits**: priority, flag, and moving a reminder to a different list,
  all from the command line.
- **Session persistence**: logs in once (Apple ID password + 2FA), then
  reuses the session via Windows Credential Manager + a persisted cookie
  jar — no repeated password/2FA prompts.

## Requirements

- Windows (uses Windows Credential Manager and the WinRT toast notification
  API; does not build on other platforms)
- Rust (stable toolchain)

## Building

```
cargo build --release
```

The first build compiles the bundled `.proto` definitions (for Apple's
"topotext" CRDT text format) using a vendored `protoc`, so no separate
Protocol Buffers installation is required.

## Usage

```
reminder-proxy-client --apple-id you@example.com <command>
```

| Command | Description |
|---|---|
| `login` | Test the login flow end-to-end. |
| `lists` | List all reminder lists. |
| `list-reminders <list_id>` | List reminders in one list. |
| `create <list_id> <title>` | Create a reminder. |
| `set-priority <reminder_id> <priority> [--flagged]` | Set priority (0 none, 1 high, 5 medium, 9 low) and flag. |
| `move <reminder_id> <target_list_id>` | Move a reminder to a different list. |
| `reorder <list_id> <reminder_id>...` | Rewrite a list's manual sort order (full new order, space-separated). |
| `delete <reminder_id>` | Soft-delete a reminder. |
| `watch [--interval-secs <n>]` | Poll for due reminders and fire toast notifications (default: every 300s). |
| `test-notify` | Fire a test toast notification. Does not touch iCloud data. |

On first run you'll be prompted for your Apple ID password and a 2FA code
(sent to a trusted device). Subsequent runs reuse the persisted session
automatically.

**Reordering** takes the *complete* new order for a list — build it from the
current order returned by `lists`/`list-reminders`, don't hand-write it from
scratch.

## How it works

See [`proto/`](proto/) for Apple's CRDT text format definitions, and the
module docs in `src/` (`srp.rs`, `auth.rs`, `cloudkit.rs`, `reminders.rs`,
`crdt.rs`) for the reverse-engineered protocol details:

- Login uses the same `idmsa.apple.com` SRP6a + 2FA flow icloud.com's web
  frontend uses (not the GSA/anisette flow used by sideloading tools like
  AltServer — that's a different, unrelated Apple auth system).
- Reminders/list data lives in a CloudKit database (`ckdatabasews`
  webservice, container `com.apple.reminders`), reached via `/records/query`,
  `/records/lookup`, `/records/modify`, and `/changes/zone`.
- Reminder titles/notes are encoded as Apple's proprietary "topotext" CRDT
  document format (protobuf + zlib + base64) — plain strings are rejected.

## License

MIT — see [LICENSE](LICENSE).
