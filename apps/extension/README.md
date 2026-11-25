# Parlance tab audio (Chromium extension)

Captures the audio of one browser tab (Meet, Teams web, Zoom web…) and streams it to the Parlance desktop
app for on-device transcription. See [ADR 0010](../../docs/adr/0010-browser-tab-bridge.md).

## Build

```sh
pnpm --filter @parlance/extension build      # outputs apps/extension/dist
pnpm --filter @parlance/extension test       # vitest: DSP, encoding and message guards
```

## Install for development

1. Open `chrome://extensions` (or `edge://extensions`), enable **Developer mode**, click **Load unpacked** and pick
   `apps/extension/dist`.
2. Copy the extension **ID** shown on the card (32 letters `a`–`p`). Unpacked IDs depend on the folder path, so the
   ID changes if you load the extension from somewhere else.
3. Build and register the native messaging host for that ID:

   ```sh
   cargo build --release -p parlance-native-host
   target/release/parlance-native-host --install <extension-id>
   ```

   This writes `app.parlance.native_host.json`, which allows only `chrome-extension://<extension-id>/`:
   - **Windows**: next to the executable, referenced from
     `HKCU\Software\Google\Chrome\NativeMessagingHosts\app.parlance.native_host` and
     `HKCU\Software\Microsoft\Edge\NativeMessagingHosts\app.parlance.native_host`.
   - **macOS**: `~/Library/Application Support/{Google/Chrome,Microsoft Edge}/NativeMessagingHosts/`.
   - **Linux**: `~/.config/{google-chrome,microsoft-edge}/NativeMessagingHosts/`.

   Re-run it if you move the executable or the extension ID changes.

4. Open Parlance, go to the meeting tab, click the Parlance toolbar icon and press **Start capture**.

## Data flow

```
popup ──start──▶ service worker ──getMediaStreamId(tab)──▶ offscreen document (USER_MEDIA)
                      │                                         │ getUserMedia(chromeMediaSource: "tab")
                      │                                         │ stream ──▶ AudioContext.destination (tab stays audible)
                      │                                         │ stream ──▶ AudioWorklet: downmix ▸ sinc resample
                      │                                         │            to 16 kHz ▸ 100 ms PCM16 LE frames
                      │◀──────── base64 frames (runtime port) ──┘
                      │
                      └─ connectNative("app.parlance.native_host")
                            hello ▸ audio… ▸ end            (stdio, u32 length + JSON, ≤ 1 MiB)
                                  │
                         parlance-native-host
                            reads %LOCALAPPDATA%/parlance/bridge.json { port, token }
                            TCP 127.0.0.1:<port>: BridgeHello { token, tabTitle } ▸ audio… ▸ end
                                  │
                           Parlance desktop app
```

- Capture starts only from the popup button (a user gesture; `activeTab` grants `tabCapture` for that tab).
- The service worker owns the native port and the frame `sequence`; the offscreen document owns the media graph.
- If Parlance is not running, the host replies `unavailable` and the popup shows why. Stopping, closing the tab or
  the host disconnecting tears down the offscreen document and sends `end`.
- Wire types come from `apps/desktop/src/bindings` (generated from `crates/protocol`); only `sequence` is narrowed
  from `bigint` to `number` because JSON cannot carry a `bigint`.

## Permissions

`tabCapture`, `offscreen`, `nativeMessaging`, `activeTab`. No host permissions, no content scripts, and the
extension pages CSP only allows the extension's own scripts.
