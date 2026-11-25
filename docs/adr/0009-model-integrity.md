# 0009 — Pinned SHA-256 and staged model downloads

**Context.** Models and the LLM runtime are large binaries fetched from Hugging Face and GitHub. A truncated
or tampered file must never be loaded, and the runtime is an executable.

**Decision.** The catalog pins URL, byte size and SHA-256 for each artifact (from Hugging Face LFS metadata and
GitHub release digests). Downloads stream into `.downloads/*.part` while hashing, abort early on size overrun,
and are renamed into place only after the digest matches. Archives are extracted into a staging directory and
renamed atomically, with an `.installed` marker. Downloads can be cancelled.

**Consequences.** Upgrading a model is a reviewed code change. Listing is cheap (size and marker checks);
integrity is enforced at install time.
