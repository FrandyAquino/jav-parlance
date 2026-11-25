# Local patches to whisper-rs-sys 0.15.0

Vendored from crates.io because the upstream build script passes the MSVC-only
`/utf-8` flag to every Windows C++ compiler, which breaks `x86_64-pc-windows-gnu`
(MinGW) builds.

- `build.rs`: add `/utf-8` only when `CARGO_CFG_TARGET_ENV == "msvc"`.

Remove this directory and the `[patch.crates-io]` entry once upstream ships the fix.
See `docs/adr/0006-vendored-whisper-rs-sys.md`.
- `build.rs`: on Windows targets, retype generated enum aliases from `c_int` to
  `c_uint` (whisper-rs assumes Linux enum signedness; same size and ABI).
- `build.rs`: under MinGW, copy `ggml*.a` to `libggml*.a` so rustc finds them.
- `build.rs`: link libstdc++ and winpthread statically under MinGW.
- `build.rs`: disable `GGML_NATIVE` on Apple targets so packaged builds do not require the build machine's CPU.
