# Releasing micro-moka

This repository is configured for release-on-merge to `main`.

## One-time setup

1. Add repository secret `CARGO_REGISTRY_TOKEN`.
2. In branch protection for `main`, require this check:
   - `Validate release readiness`
3. Keep merge policy as pull-request merge into `main`.

## Per-PR requirements

Every PR targeting `main` must:

1. Bump `[package].version` in `Cargo.toml`.
2. Add the matching changelog header in `CHANGELOG.md`:

```md
## [x.y.z] - YYYY-MM-DD
```

3. Pass `Release Check` workflow (runs `cargo publish --dry-run --locked`).

## What happens on merge to `main`

`Publish Crate` workflow runs automatically:

1. Reads crate `name` and `version` from `Cargo.toml`.
2. Checks whether that version already exists on crates.io.
3. Publishes with `cargo publish --locked` if it is new.
4. Ensures git tag `v<version>` exists.
5. Creates a GitHub release from the matching changelog section.

The publish workflow is idempotent: re-runs will skip publishing if crates.io already has that version.
