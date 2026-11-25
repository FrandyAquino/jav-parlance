# 0008 — WASAPI capture with fixed format and silence filling

**Context.** Windows capture must cover endpoint loopback, process loopback (Windows 10 build 20348+ and
Windows 11) and microphones, survive device changes, and never glitch other applications.

**Decision.** Use the `wasapi` crate on a dedicated MTA thread in event-driven shared mode, requesting f32
48 kHz stereo with `autoconvert` so the rest of the pipeline sees one format. The thread only copies into the
lock-free ring, dropping and counting samples on overflow. Loopback delivers no packets during silence, so the
thread injects zeros for the elapsed time to keep timestamps aligned with wall-clock time. `system:default` and
`mic:default` follow the default device and reopen it on loss (up to 40 attempts); process loopback ends with
its process. Process loopback availability is detected from the OS build number in the registry.

**Consequences.** Consumers stay simple and transcript timestamps survive pauses. The autoconvert resampler is
adequate for speech. A microphone denied by Windows privacy settings maps to `PermissionDenied` with guidance.
