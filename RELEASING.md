# Releasing

A release publishes six packages to crates.io and creates a GitHub release. The pipeline (`.github/workflows/release.yml`) does the work; the maintainer prepares the version and pushes a tag.

## The packages

| Package               | Folder            | Version example |
| --------------------- | ----------------- | --------------- |
| `hwcodec`             | repository root   | `0.1.0`         |
| `hwcodec-libvpx-src`  | `sys/libvpx-src`  | `0.1.0+1.15.2`  |
| `hwcodec-svt-av1-src` | `sys/svt-av1-src` | `0.1.0+4.3.0`   |
| `hwcodec-dav1d-src`   | `sys/dav1d-src`   | `0.1.0+1.5.4`   |
| `hwcodec-libvpl-src`  | `sys/libvpl-src`  | `0.1.0+2.17.0`  |
| `hwcodec-prebuilt-x86_64-pc-windows-msvc` | `sys/prebuilt-x86_64-pc-windows-msvc` | `0.1.0+native-libs.1` |

All six always carry the same version. The four source packages add the version of the library they bundle after a plus sign; that part only changes when the bundled library is updated. The package with the prebuilt Windows libraries adds the `native-libs` release its libraries come from (see "Windows: the prebuilt codec libraries").

## Steps for a release

1. **Set the version** in all six `Cargo.toml` files (`version = "X.Y.Z"`, for the source packages `X.Y.Z+<library version>`, for the prebuilt Windows libraries `X.Y.Z+native-libs.<number>`), and in the root `Cargo.toml` the five requirements on those packages (`hwcodec-… = { version = "X.Y.Z", … }`).
2. **Update `CHANGELOG.md`:** rename `## [Unreleased]` to `## [X.Y.Z] - YYYY-MM-DD` and put a new, empty `## [Unreleased]` above it. The text of the `[X.Y.Z]` section becomes the notes of the GitHub release.
3. **Commit** (`chore: release X.Y.Z`), push to `main` and wait for CI to be green.
4. **Rehearse** (optional, recommended): Actions → Release → Run workflow, leave `dry_run` switched on. This checks and packages everything and publishes nothing.
5. **Tag and push:** `git tag vX.Y.Z && git push origin vX.Y.Z`. The tag must sit on the release commit on `main`; the release workflow refuses a tag whose commit is not part of `main`. Pushing the tag starts the real release.
6. **Check the result:** the six packages on crates.io, the page on docs.rs, and the release on GitHub.

Published versions can never be replaced or removed from crates.io (only "yanked", which hides them from new users). If something is wrong after publishing, fix it and release the next patch version.

## Windows: the prebuilt codec libraries

On Windows the crate's build does not compile the bundled sources. It links libvpx, SVT-AV1, dav1d and the oneVPL dispatcher prebuilt for `x86_64-pc-windows-msvc`.

- **Origin:** one archive in a GitHub release of this repository named `native-libs-<number>` (file `hwcodec-native-libs-<number>-x86_64-pc-windows-msvc.tar.gz`), built by the workflow "Windows codec libraries". These releases are separate from the crate versions (`vX.Y.Z`).
- **The pin:** `PREBUILT_TAG` and `PREBUILT_SHA256` in `build.rs`.
- **On crates.io:** the package `hwcodec-prebuilt-x86_64-pc-windows-msvc` contains the unpacked archive in `prebuilt/`. That folder is **not in git**. The pipeline fills it from the pinned archive (`.github/scripts/fetch-windows-prebuilt.sh`), packs it and publishes it only if the packed files are byte for byte the archive's (`.github/scripts/verify-prebuilt-package.sh`). Because cargo only packs files outside git with `--allow-dirty`, the pipeline uses that switch for this one package and checks itself that the checkout otherwise equals the tag.
- **In a git checkout** the package is empty and the build downloads the pinned archive instead.
- **Never delete or change a `native-libs-*` release.** Published crate versions point at them: a build from a git checkout of that version downloads exactly that file, and the release is the documented origin of the binaries inside the published package.
- **The numbers move together:** the package version ends in the number of the release it holds. `PREBUILT_TAG = "native-libs-2"` goes with `version = "X.Y.Z+native-libs.2"` in `sys/prebuilt-x86_64-pc-windows-msvc/Cargo.toml`. The pipeline stops if they differ.

A new archive is needed whenever a library under `sys/` is updated, or `.github/scripts/build-windows-libs.sh` changes how the libraries are built. CI and the release pipeline fail while the pinned archive holds other library versions than `sys/`. Publishing a new archive:

1. On the branch with the change: Actions → Windows codec libraries → Run workflow, choose the branch, leave `tag` empty. This only builds; check that it is green.
2. Run it again with `tag` set to the next free number, e.g. `native-libs-2`. This creates the release with the archive, a `.sha256` file and a description. The summary of the run shows the SHA-256.
3. On the same branch, in one commit: put the new tag and SHA-256 into `PREBUILT_TAG` and `PREBUILT_SHA256` in `build.rs`, change the package version in `sys/prebuilt-x86_64-pc-windows-msvc/Cargo.toml` to `…+native-libs.<number>`, and update the release name and address in the README (section "Building on Windows") and in `sys/prebuilt-x86_64-pc-windows-msvc/README.md`.
4. CI on Windows must be green before the branch is merged: it builds once with the package filled from the new archive and once with the download.

## What the pipeline does

1. **Check:** the tag equals the version of all six packages, and `CHANGELOG.md` has a section for that version. Otherwise it stops before anything is published.
2. **Tests:** the complete CI checks (formatting, Clippy, tests, documentation, examples, licence check, package check) on Linux, macOS and Windows, including one Windows build with the prebuilt libraries in the package and one that downloads them. In parallel it checks that the prebuilt Windows libraries pinned in `build.rs` can be downloaded, have the pinned checksum, hold the library versions of this release and match the package version.
3. **Publish to crates.io** in dependency order: `hwcodec-libvpx-src`, `hwcodec-svt-av1-src`, `hwcodec-dav1d-src`, `hwcodec-libvpl-src`, `hwcodec-prebuilt-x86_64-pc-windows-msvc`, then `hwcodec`. The package with the prebuilt Windows libraries is filled from the pinned archive and compared with it first. After each package it waits until crates.io offers it for download. A version that is already on crates.io is skipped, so a release that stopped half-way can simply be run again (Actions → the failed run → Re-run failed jobs).
4. **GitHub release** for the tag, with the changelog section as notes.

A **dry run** runs the job "Package (dry run)", which has no access to crates.io; a **real publish** runs the job "Publish to crates.io" in the `release` environment, which accepts version tags only. A real publish only happens from a version tag; a run started by hand from a branch refuses to publish even with `dry_run` switched off.

**crates.io's limit for new package names.** crates.io limits how fast one account may publish brand-new crates: a burst of 5 new crates at once, then 1 more every 10 minutes ([Publishing Rate Limits](https://crates.io/docs/rate-limits)). This only matters when a release introduces new package names, as 0.1.0 did with six; new versions of existing packages may be published 30 at once, then 1 per minute. If the job "Publish to crates.io" fails with `429 Too Many Requests: You have published too many new crates in a short period of time. Please try again after …`, wait until the time given in the message and re-run only the failed job (Actions → the failed run → Re-run failed jobs). The versions already published are skipped.

## One-time setup (maintainer)

### The first release: a crates.io token

crates.io can only trust this repository for a package once the package exists. The very first publish therefore uses a crates.io API token. It is stored as the organisation secret `CARGO_REGISTRY_TOKEN`, which this repository can read; nothing has to be set up for the first release. The token needs the scopes `publish-new` and `publish-update`.

### After the first release: Trusted Publishing

With Trusted Publishing, crates.io accepts a publish because it comes from this repository's release workflow; no stored token is needed. Register it once per package, for all six: `hwcodec-libvpx-src`, `hwcodec-svt-av1-src`, `hwcodec-dav1d-src`, `hwcodec-libvpl-src`, `hwcodec-prebuilt-x86_64-pc-windows-msvc` and `hwcodec`.

1. On crates.io open the package → Settings → Trusted Publishing → Add, publisher GitHub.
2. Fill in: repository owner `Mumcord`, repository name `hwcodec`, workflow filename `release.yml`, environment `release`.
3. When all six are registered, take this repository off the organisation secret `CARGO_REGISTRY_TOKEN` (organisation Settings → Secrets and variables → Actions → the secret → repository access), or delete the secret and revoke the token on crates.io if nothing else uses it.

From then on the pipeline asks crates.io for a token that is valid for 30 minutes on each release. As long as this repository can read the secret `CARGO_REGISTRY_TOKEN`, the pipeline uses it instead.

Trusted Publishing only works for packages that already exist. A release that adds a new package name needs a crates.io token with the scope `publish-new` again (give this repository access to the secret for that release), and the new package is registered for Trusted Publishing afterwards.

It is also worth adding a second owner to each package on crates.io (package → Settings → Owners), so that the packages do not depend on one account.

### Claude review

The workflow `.github/workflows/claude-review.yml` reviews pull requests from branches of this repository. It uses the organisation secret `CLAUDE_CODE_OAUTH_TOKEN` (created with `claude setup-token`). If the token is missing or expired the review is skipped with a warning; pull requests do not fail because of it.

A review without findings approves the pull request; a review with findings requests changes. A pull request that changes files under `.github/` is reviewed but never approved automatically. The workflow never merges.
