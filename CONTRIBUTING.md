# Contributing to Parlance

Fork the repository, create a branch, and open a pull request against `main`.
Keep changes focused and describe the problem, the resulting behavior, and how you
tested it. For substantial features, open an issue first to discuss the approach.

## Local checks

Install the prerequisites in the [README](README.md#develop) and the platform
guide for [Windows](docs/platform/windows.md), [macOS](docs/platform/macos.md), or
[Linux](docs/platform/linux.md). Use the pnpm version in `package.json`.

```sh
pnpm install --frozen-lockfile
pnpm --filter @parlance/desktop build
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
pnpm typecheck
pnpm lint
pnpm test
pnpm --filter @parlance/extension build
```

Rust tests regenerate TypeScript contracts in `apps/desktop/src/bindings`. Include
changed and newly generated contracts in the same PR as their Rust definitions.
When changing dependencies intentionally, update and commit the relevant lockfile.

Add or update tests for behavior changes. For capture, GPU, permissions, and desktop
integration changes, also describe the OS and hardware you actually tested. A
successful build does not establish that microphone or system capture works.

## What CI checks

Every PR, including PRs from forks, runs:

- Rust formatting, Clippy, tests, and generated-contract checks on Linux, macOS,
  and Windows.
- Type checking, lint, tests, and builds for the desktop frontend and extension.
- Installer builds for Linux, macOS, and Windows after the checks pass.
- `CI required`, which succeeds only if all those jobs succeed.

Installer artifacts expire after seven days. They are test builds, not signed and
notarized public releases. CI does not require contributor secrets or downloaded
AI models. GitHub may ask a maintainer to approve a fork's workflow run.

## Maintainer setup

In GitHub **Settings → Rules → Rulesets** (or branch protection), protect `main`:

- Require a pull request and resolution of review conversations.
- Require successful checks from GitHub Actions and an up-to-date branch.
- Block force pushes and branch deletion.

Until the workflow containing `CI required` has run and is merged, use the existing
checks: `Frontend`, `Rust (ubuntu-latest)`, `Rust (macos-latest)`, and
`Rust (windows-latest)`. Then replace those required checks with `CI required`, so
installer failures also block merging. Do not require a check that does not yet
exist on the target branch. If using a merge queue later, add the `merge_group`
workflow trigger before enabling it.

Keep PR workflows on GitHub-hosted runners, with read-only token permissions and
no secrets. Do not use `pull_request_target` to execute contributor code. Review
workflow changes before approving runs. Automated checks complement maintainer
review; they do not review product intent, privacy, or architecture.
