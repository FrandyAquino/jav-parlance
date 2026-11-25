# Windows

Status: **verified** on Windows 11 (Intel laptop CPU, GTX 1650 4 GB). WASAPI endpoint loopback → Silero VAD →
streaming Whisper base Q8 works end-to-end in the desktop UI, and the `llama-server` b12686 Vulkan sidecar with
Qwen3.5-2B Q4_K_M translates and drafts copilot replies.

## Toolchain

Either toolchain works. MSVC is the simplest if you have the Visual Studio C++ workload; the GNU setup below needs
no Visual Studio and no admin rights.

### MSVC (`x86_64-pc-windows-msvc`)

Install Rust, the VS C++ build tools, CMake and LLVM (for bindgen's `libclang`). Nothing else is project-specific.

### GNU + portable MinGW (`x86_64-pc-windows-gnu`)

1. Toolchain, pinned per checkout (do not commit a `rust-toolchain.toml`; it breaks other platforms):

   ```powershell
   rustup toolchain install stable-x86_64-pc-windows-gnu
   cd parlance
   rustup override set stable-x86_64-pc-windows-gnu
   ```

2. MinGW: unpack a WinLibs MinGW-w64 build (GCC, `mingw32-make`, CMake, Ninja) to e.g.
   `%USERPROFILE%\mingw64` and put its `bin` on the user `PATH`.
3. libclang for bindgen without LLVM:

   ```powershell
   pip install --user libclang
   # LIBCLANG_PATH = the wheel's clang\native directory (contains libclang.dll)
   setx LIBCLANG_PATH "$(python -c "import clang, os; print(os.path.join(os.path.dirname(clang.__file__), 'native'))")"
   ```

4. Point clang at the MinGW headers (replace `<ver>` with the GCC version folder, e.g. `16.1.0`):

   ```powershell
   setx BINDGEN_EXTRA_CLANG_ARGS "-isystem C:/Users/<you>/mingw64/lib/gcc/x86_64-w64-mingw32/<ver>/include -isystem C:/Users/<you>/mingw64/x86_64-w64-mingw32/include"
   ```

   Open a new shell afterwards (`setx` does not affect running shells).

What the repo already does for MinGW:

- `.cargo/config.toml` adds `-C target-feature=+crt-static` for `x86_64-pc-windows-gnu`, so binaries do not load
  `libstdc++`/`winpthread` DLLs (Git for Windows ships an incompatible `libstdc++-6.dll` that would otherwise
  shadow WinLibs at runtime and fail with `STATUS_ENTRYPOINT_NOT_FOUND`).
- `third_party/whisper-rs-sys` is vendored via `[patch.crates-io]` with target-gated fixes (see its `PATCHES.md`
  and ADR 0006): `/utf-8` only for MSVC, unsigned enum aliases on Windows, `lib` prefix for ggml archives,
  static libstdc++/winpthread.
- `apps/desktop/src-tauri` sets `test = false`: Windows test executables lack the Common Controls v6 manifest
  Tauri needs.

Check: `cargo test --workspace` then `pnpm install && pnpm dev`.

## Capture (`crates/capture/src/windows`)

| Source id | Meaning | Notes |
| --- | --- | --- |
| `system:default` | Loopback of the default output; follows default-device changes | Reconnects up to 40 × 500 ms |
| `system:<endpoint id>` | Loopback of one render endpoint | Reconnects |
| `app:<pid>` | Process loopback of that process **tree** (`include_tree = true`) | Listed only on build ≥ 20348; no reconnect |
| `mic:default`, `mic:<endpoint id>` | Capture endpoint | Reconnects |

- `wasapi` 0.27 on a dedicated thread with COM MTA, event-driven shared mode, 20 ms buffer
  (`buffer_duration_hns: 200_000`), `autoconvert: true` to f32 48 kHz stereo, so the rest of the pipeline sees one
  format (ADR 0008).
- Applications are listed from audio sessions on the default output (non-expired, deduplicated by pid); names come
  from the process image (`QueryFullProcessImageNameW`), the session display name, or "System sounds".
- Silence filling: when no packets arrive for ≥ 100 ms, zeros for the elapsed time are pushed.
- The capture thread only copies into the ring; overflow drops and counts samples (see
  [concurrency](../architecture/concurrency.md)).

### Process loopback build requirement

Process loopback (`AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK`) needs Windows 10 build **20348** or later
(Windows 11, Server 2022). The adapter reads `CurrentBuildNumber` from
`HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion`; below that, `CaptureSupport.application` is `false` and no
`app:` sources are listed. Consumer Windows 10 22H2 (19045) is therefore endpoint-loopback only.

### Microphone privacy

If Windows blocks microphone access, `IAudioClient::Initialize` returns `E_ACCESSDENIED`, which maps to
`PermissionDenied` ("microphone access is disabled in Windows privacy settings"). Fix: Settings → Privacy &
security → Microphone → enable **Microphone access** and **Let desktop apps access your microphone**.
Loopback capture needs no permission.

## LLM runtime

Catalog entry `llama-server` = `llama-b12686-bin-win-vulkan-x64.zip` (SHA-256 pinned). It is extracted under
`%LOCALAPPDATA%\parlance\models\llama-server\` and started with `CREATE_NO_WINDOW`, its working directory set to
the binary's folder (backend DLLs load from there), and assigned to a Job Object with `KILL_ON_JOB_CLOSE`.
Vulkan needs a GPU driver with Vulkan support; otherwise the supervisor falls back to CPU after two failed starts.

## Paths

| What | Where |
| --- | --- |
| Settings | `%APPDATA%\parlance\settings.json` |
| Models | `%LOCALAPPDATA%\parlance\models` |
| History (opt-in) | `%LOCALAPPDATA%\parlance\history.db` |
| Bridge discovery | `%LOCALAPPDATA%\parlance\bridge.json` |

## Window

`decorations: false`, `transparent: true`, `alwaysOnTop: true`, window effects `mica`/`acrylic` (WebView2).
The UI keeps translucent surfaces (`data-effects="native"`).
