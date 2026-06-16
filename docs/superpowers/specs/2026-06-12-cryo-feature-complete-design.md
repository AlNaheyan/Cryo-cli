# Cryo — Feature-Complete CLI Design

Date: 2026-06-12
Status: Approved

## Goal

Finish Cryo as a feature-complete, installable command-line tool for secure file
sharing over the Firefox Send protocol (via `ffsend-api`). Today it wraps five
commands with hard-coded behavior and CWD-relative state. This work closes the
remaining ffsend feature gaps, makes the binary correct when installed and run
from anywhere, hardens error handling, and adds tests and packaging.

Out of scope: splitting into a reusable library crate; running our own Send
server; release/CI automation.

## Features

1. **Custom limits & expiry** — replace the hard-coded `download_limit = 5` /
   `expiry_time = 3600` in `upload.rs` with `--downloads` and `--expiry` flags.
2. **Password protection** — `--password` on upload; download/info prompt for or
   accept a password when a file is protected.
3. **Configurable host** — global `--host` flag (default `https://send.vis.ee/`,
   overridable via `CRYO_HOST`) instead of the hard-coded instance.
4. **Change params after upload** — a new `params` command to change download
   limit / expiry / password on an already-uploaded file, gated on a stored
   owner token (like `delete`).

## Architecture (Approach A — focused refactor)

Extract the shared concerns that multiple commands need into small, single-purpose
modules, then keep one module per command. This also fixes the existing smell
where the owner-token helpers live in `upload.rs` and `delete.rs`/`info.rs` reach
in to import them.

```
src/
  main.rs          entry: arg-count branch -> menu or clap dispatch; defines global --host
  menu.rs          interactive terminal-menu mode (extracted from main.rs)
  config.rs        resolves config dir, token-file path, default host, download dir
  client.rs        builds the ffsend client for a given host URL
  token_store.rs   OwnerToken type + load/save/get (moved out of upload.rs)
  commands/
    mod.rs
    upload.rs      flags: --downloads, --expiry, --password
    download.rs    flag: --password, --out; writes to resolved download dir
    exists.rs      mostly unchanged
    info.rs        graceful errors (no .expect panics); --password
    delete.rs      uses token_store
    params.rs      NEW: change downloads/expiry/password on an uploaded file
```

### Shared modules

- **`config.rs`** — uses the `directories` crate to resolve `~/.config/cryo/`
  (OS-appropriate) for `owner_token.json`. Resolves the default download
  directory (OS Downloads dir) and exposes the default host. The download dir is
  overridable per-invocation with `--out`.
- **`client.rs`** — single helper `build(host: &Url) -> Client` that constructs
  the `ffsend-api` client config and client for a given host. Replaces the
  duplicated `ClientConfigBuilder::default().build()?.client(true)` in every
  command.
- **`token_store.rs`** — owns `OwnerToken` and `load()` / `save(file_id, token)`
  / `get(file_id)`. Backed by the JSON map in the config dir. Replaces
  `save_token` / `read_tokens_from_file` currently in `upload.rs`.

### Host resolution

`--host` is a global flag with default `https://send.vis.ee/`. If the flag is
absent, fall back to the `CRYO_HOST` env var, then the compile-time default. The
resolved host URL is threaded into `client::build` and into any action that needs
it (upload/download pin a protocol `Version::V3`). No persistent TOML config file.

### State migration

On startup, the token store reads from `~/.config/cryo/owner_token.json`. If a
legacy `./owner_token.json` exists in the working directory (the old location),
its entries are merged in on load so existing owner tokens are not lost. New
writes go only to the config-dir location.

## Data flow

- **Upload:** resolve host -> `client::build` -> build `ParamsData` from
  `--downloads`/`--expiry` -> attach optional `--password` -> `Upload::invoke`
  -> print share URL -> `token_store.save(id, owner_token)`.
- **Download:** `client::build` -> parse URL -> fetch metadata (supply password
  if the file is protected) -> download into the resolved download dir (or
  `--out`) using the original file name -> print path.
- **Info:** fetch public metadata (graceful error on failure); if a matching
  owner token exists, also fetch management info (download count/limit/left);
  supply password if protected.
- **Delete:** parse URL -> `token_store.get(id)` (error if missing) -> set owner
  token -> `Delete::invoke`.
- **Params (new):** parse URL -> `token_store.get(id)` (error if missing) -> set
  owner token -> apply the requested changes to download limit / expiry /
  password.

### Password handling

A password-protected file fails the metadata fetch without a password. Commands
that read a file (`download`, `info`) accept `--password`; if the file is
protected and no password was given, prompt for it securely with `rpassword`
(no echo) rather than failing outright. The exact `ffsend-api` password API is
confirmed during planning/implementation; this spec fixes the behavior, not the
call signatures.

## Error handling

Replace `.expect()` / panics — most acute in `info.rs`, which currently panics on
a bad URL or any network error — with `Result`-returning worker functions. Adopt
`anyhow` for ergonomic error context across commands, and have `main` return a
non-zero process exit code on failure. The thin `*_cmd` layer reports errors via
`eprintln!`.

## Testing

- **Unit (no network):** `token_store` save/load/get round-trip in a temp dir;
  config path resolution; building `ParamsData` from flags; host/URL parsing and
  the `CRYO_HOST` fallback. Worker functions stay thin so this logic is testable
  without the network.
- **Integration (opt-in):** a small number of `#[ignore]`d end-to-end tests
  against a real Send instance, run only on demand. Never part of the default
  `cargo test` run.

## Packaging & loose ends

- Drive clap's `version` from `env!("CARGO_PKG_VERSION")` instead of the stale
  hard-coded `"0.1"` string in `main.rs`.
- Update `README.md` with the new flags and the `params` command.
- Install path is `cargo install --path .`. Release/CI automation is out of scope.

## New dependencies

- `directories` — OS-appropriate config/download directory resolution.
- `rpassword` — secure (no-echo) password prompts.
- `anyhow` — ergonomic error propagation and context.

## Interactive menu

The menu stays but moves to `menu.rs`. It keeps simple flows: uploads use default
params (power users reach for flags), and download/info prompt for a password
when the target file is protected.
