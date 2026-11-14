# Contributing

Thanks for helping. Hardware test reports are as valuable as code: see "Reporting hardware results" in the [README](README.md).

## Before you open a pull request

- Open an issue first for anything larger than a small fix, so the direction can be agreed.
- Run `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, `cargo test` and `cargo doc --no-deps`. CI runs the same checks on Linux, Windows and macOS.
- Keep changes small and focused, one logical change per commit, with a clear message ([Conventional Commits](https://www.conventionalcommits.org/) style: `feat:`, `fix:`, `docs:`).
- New logic needs tests. A bug fix starts with a test that reproduces the bug.
- Add a line under `[Unreleased]` in `CHANGELOG.md` for every user-visible change.
- Public items need documentation; the documentation build must stay free of warnings.

## Code rules

- Never block, allocate, lock or log in code that runs per frame on a real-time path unless it is unavoidable and documented.
- No `unwrap` in library code; report errors through `Error` or log them.
- Comments explain why, not what.

## Licence

By submitting a contribution you agree that it is licensed under `MIT OR Apache-2.0`, the same as the crate, without additional terms. Third-party sources and headers under `vendor/` and in the bundled source packages keep their own licences.

Releases are described in [RELEASING.md](RELEASING.md).
