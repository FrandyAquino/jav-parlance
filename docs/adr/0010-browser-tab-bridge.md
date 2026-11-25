# 0010 — Browser tab audio via MV3 extension + native messaging

**Context.** Web meetings (Meet, Teams web, Zoom web) run in a single browser tab. Capturing only that tab
avoids other system sounds and works where process loopback is unavailable.

**Decision.** A Manifest V3 Chromium extension captures the active tab with
`chrome.tabCapture.getMediaStreamId`, processes it in an offscreen document with an AudioWorklet (downmix to
16 kHz PCM16), and streams frames over native messaging to `parlance-native-host`, which forwards them to the
running desktop app over a local IPC endpoint. The host manifest allow-lists only the extension id, frames are
capped at 1 MiB and strictly typed (`protocol::native_messaging`), and capture starts only from a user gesture.

**Consequences.** Works without system-wide capture permissions. The desktop app must install the host manifest
per browser and OS.
