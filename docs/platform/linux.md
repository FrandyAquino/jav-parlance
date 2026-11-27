# Linux

Status: **Linux builds and isolated lifecycle tests exist; real desktop audio acceptance is pending.**
The lifecycle tests use a synthetic `parec` child to verify reconnection and cleanup. They do not establish
capture behavior on a real PulseAudio or PipeWire desktop, or compatibility across CPU baselines.

## Adapter design (`crates/capture/src/linux`)

The adapter speaks the PulseAudio protocol, which both PulseAudio and PipeWire (through `pipewire-pulse`) serve. It
drives the standard command-line tools instead of linking `libpulse`, so the build needs no audio dev headers and
capture runs out of process.

| Source id | Discovery | Capture command |
| --- | --- | --- |
| `system:default` | always listed | `parec --device=@DEFAULT_MONITOR@` |
| `system:<sink>` | `pactl --format=json list sinks` | `parec --device=<sink>.monitor` |
| `app:<sink-input index>` | `pactl --format=json list sink-inputs` (`application.name`, `application.process.id`) | `parec --monitor-stream=<index>` |
| `mic:default` | always listed | `parec --device=@DEFAULT_SOURCE@` |
| `mic:<source>` | `pactl --format=json list sources`, monitors excluded | `parec --device=<source>` |

- `parec --raw --format=float32le --rate=48000 --channels=2 --latency-msec=20 --client-name=Parlance`.
- `parlance-parec` thread reads 10 ms chunks from stdout, converts to f32 and pushes into the ring.
- When `parec` exits, it is restarted after 500 ms, up to 40 times, emitting `DeviceLost`/`Reconnected`.
  `app:` sources do not reconnect.
- Capabilities are reported only if `pactl info` succeeds.
- `pactl --format=json` requires PulseAudio 16+ or pipewire-pulse.
- Dropping the stream kills and reaps the `parec` child.

Native PipeWire (`pw_stream` with `stream.capture.sink`) is the documented long-term option
(`docs/research/platform-capture.md`) but is not implemented.

## Required packages

Runtime:

- `pulseaudio-utils` (provides `pactl` and `parec`), plus either PulseAudio or PipeWire with `pipewire-pulse`.
- WebKitGTK 4.1 runtime (Tauri webview).
- A Vulkan driver for GPU offload of `llama-server` (`llama-b12686-bin-ubuntu-vulkan-x64.tar.gz`); without one the
  supervisor falls back to CPU. The binary links system glibc; test on the oldest distro you support.

Build (Debian/Ubuntu):

```sh
sudo apt-get install -y libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libclang-dev cmake build-essential
```

## Desktop specifics

- The UI sets `data-effects="none"` on Linux: compositors give the webview no backdrop blur, so surfaces are opaque.
- Paths follow XDG via `directories`: models in `~/.local/share/parlance/models`, settings in
  `~/.config/parlance/settings.json`, bridge discovery in `~/.local/share/parlance/bridge.json` (mode `0600`).
- `llama-server` uses the Unix lifetime-pipe guardian: closing the owner's pipe, including after a crash,
  terminates the sidecar. Synthetic process tests cover this mechanism; real Linux desktop acceptance remains pending.

## Verification checklist (not done yet)

- [ ] `parlance-cli sources` lists sinks, sink-inputs and mics on PulseAudio and on PipeWire.
- [ ] `parlance-cli listen --source system:default --seconds 20` transcribes.
- [ ] `app:<index>` captures only that stream and ends cleanly when the app stops.
- [ ] Default-sink switch while capturing (`@DEFAULT_MONITOR@` follows it after a `parec` restart?).
- [ ] Desktop UI end-to-end, including the Vulkan sidecar.
