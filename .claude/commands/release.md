---
description: Bump the version, commit, tag and push to trigger the release workflow
argument-hint: <X.Y.Z | patch | minor | major>
allowed-tools: Bash, Read, Edit
---

You are preparing and triggering an Aurora release. Argument provided: `$ARGUMENTS`.

The version lives in the root `Cargo.toml` (`[workspace.package] version`), all
crates inherit it. The `.github/workflows/release.yml` workflow is triggered on push
of a `v*` tag.

Execute the steps in order. **Stop immediately and explain** if a
check fails, without committing or pushing anything.

## 1. Compute the target version

- Read the current version: the `version = "X.Y.Z"` line at the start of `Cargo.toml`.
- Depending on `$ARGUMENTS`:
  - `X.Y.Z` or `vX.Y.Z` (exact semver) -> use this number (without the `v`).
  - `patch` -> increment Z. `minor` -> increment Y, Z=0. `major` -> increment X, Y=0, Z=0.
  - empty or other -> **stop** and remind the usage: `/release <X.Y.Z | patch | minor | major>`.
- Note `NEW` (e.g. `0.3.0`) and `TAG = vNEW` (e.g. `v0.3.0`). Verify that `NEW` is
  strictly greater than the current version, otherwise stop.

## 2. Guardrails (everything must pass before continuing)

```bash
git status --porcelain        # must be empty (clean tree)
git rev-parse --abbrev-ref HEAD   # must be "main"
git fetch origin --tags --quiet
git rev-list --left-right --count main...origin/main   # main must not be behind
git tag -l "$TAG"             # must be empty (local tag does not exist)
git ls-remote --tags origin "$TAG"   # must be empty (remote tag does not exist)
```

If the tree is not clean, if you are not on `main`, if `main` is behind
`origin/main`, or if the tag already exists: stop and explain.

## 3. Bump the files

- `Cargo.toml`: replace the `version = "<current>"` line (start of line) with `version = "NEW"`.
- `Cargo.lock`: resynchronized via cargo (step 4, the build updates the `aurora*` entries).
- `README.md`:
  - `## Status` section: the `Project at v…` mention must point to `vNEW`.
  - Install example: `AURORA_VERSION=v…` must point to `vNEW`.

```bash
sed -i -E 's/^version = "[0-9]+\.[0-9]+\.[0-9]+"/version = "NEW"/' Cargo.toml
sed -i -E 's/(Project at v)[0-9]+\.[0-9]+(\.[0-9]+)?/\1NEW/' README.md
sed -i -E 's/(AURORA_VERSION=v)[0-9]+\.[0-9]+\.[0-9]+/\1NEW/' README.md
```

(Replace `NEW` with the actual number in the commands.)

## 4. Validate (cargo)

```bash
cargo build --release --quiet   # also updates Cargo.lock with NEW
cargo test --quiet
```

If the build or tests fail: stop, commit nothing. Then verify that the
6 `aurora*` entries of `Cargo.lock` have indeed switched to `NEW`.

## 5. Commit, tag, push

```bash
git add Cargo.toml Cargo.lock README.md
git commit -m ":bookmark: chore(release): vNEW"
git tag -a "vNEW" -m "vNEW"
git push origin main
git push origin "vNEW"
```

## 6. Report

Confirm the published version and give the tracking links:
- Workflow: `https://github.com/jdevelop-io/aurora/actions`
- Release: `https://github.com/jdevelop-io/aurora/releases/tag/vNEW`

Do not add any Claude/Anthropic attribution in the commit or the tag.
