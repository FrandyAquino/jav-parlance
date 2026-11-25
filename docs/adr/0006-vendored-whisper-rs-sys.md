# 0006 — Vendored `whisper-rs-sys` for MinGW builds

**Context.** The primary dev machine has no MSVC C++ build tools, so it builds with `x86_64-pc-windows-gnu`.
`whisper-rs-sys` 0.15.0 fails there: it passes `/utf-8` to every Windows C++ compiler, its bundled bindings
are Linux-generated, the safe wrapper assumes unsigned C enums, ggml's static libraries lack the `lib` prefix
under MinGW, and it links `libstdc++` dynamically (which then resolves to an incompatible DLL from Git for
Windows at runtime).

**Decision.** Vendor the crate in `third_party/whisper-rs-sys` with minimal, documented, target-gated patches
(`PATCHES.md`), wired through `[patch.crates-io]`. Generate bindings with a portable libclang (the pip
`libclang` wheel) plus MinGW include paths. Link the C++ runtime statically.

**Consequences.** MSVC, macOS and Linux behaviour is unchanged. We carry 13 MB of vendored C sources until the
fixes land upstream; remove the patch when they do.
