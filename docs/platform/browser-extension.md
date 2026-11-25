# Browser tab capture

Full setup and data flow: [`apps/extension/README.md`](../../apps/extension/README.md). Decision:
[ADR 0010](../adr/0010-browser-tab-bridge.md). Wire protocol: [ipc.md](../architecture/ipc.md#browser-bridge-protocol).

Status: unit and integration tests pass (extension vitest, native-host relay tests, bridge TCP tests). The
extension has **not been loaded in a real browser** yet.

## Pieces

| Piece | Path | Role |
| --- | --- | --- |
| MV3 extension | `apps/extension` | Popup → service worker → offscreen document (`USER_MEDIA`) → AudioWorklet: downmix, sinc resample to 16 kHz, 100 ms PCM16 LE frames, base64 |
| Native host | `apps/native-host` | `app.parlance.native_host`; validates and forwards frames from stdio to the desktop bridge |
| Bridge backend | `crates/capture/src/bridge.rs` | Loopback TCP listener in the desktop app; each authenticated connection is a `tab:<n>` source |

## Summary

- Permissions: `tabCapture`, `offscreen`, `nativeMessaging`, `activeTab`. No host permissions or content scripts.
  `minimum_chrome_version: 116`.
- Capture starts only from the popup's **Start capture** button (user gesture). The tab stays audible: the stream is
  also routed to `AudioContext.destination`.
- The service worker owns the native port and the frame `sequence`; only the offscreen document URL may open the
  audio port.
- If the desktop app is not running, the host answers `unavailable` with a reason the popup shows.
- Install the host per extension id: `cargo build --release -p parlance-native-host` then
  `target/release/parlance-native-host --install <extension-id>` (Chrome and Edge).

## Develop

```sh
pnpm --filter @parlance/extension typecheck
pnpm --filter @parlance/extension lint
pnpm --filter @parlance/extension test
pnpm --filter @parlance/extension build    # → apps/extension/dist, load unpacked
cargo test -p parlance-native-host -p parlance-capture
```

## Open items

- First real-browser run (Chrome, Edge) on Windows, then macOS.
- The host does not check the caller origin passed as `argv[1]`; it relies on `allowed_origins`.
- `sequence` is not checked for gaps on the desktop side.
- Brave/Chromium manifest locations are not installed.
