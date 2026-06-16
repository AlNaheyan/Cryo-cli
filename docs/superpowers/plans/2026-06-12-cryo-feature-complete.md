# Cryo Feature-Complete CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish Cryo as an installable, feature-complete CLI for the Firefox Send protocol — custom limits/expiry, password protection, configurable host, a new `params` command, OS-standard state storage, graceful errors, and tests.

**Architecture:** Approach A — extract shared concerns (`config`, `client`, `token_store`, `secret`) into small single-purpose modules, move the interactive menu to `menu.rs`, and keep one module per command. Each command worker returns `anyhow::Result<()>`; `main` returns a process exit code.

**Tech Stack:** Rust 2021, `clap` (derive), `ffsend-api` 0.7.3, `serde`/`serde_json`, `url`, `terminal-menu`, plus new deps `anyhow`, `directories`, `rpassword`.

---

## File Structure

```
src/
  main.rs          entry: arg-count branch -> menu or clap dispatch; ExitCode; clap version from env
  menu.rs          interactive terminal-menu mode (moved out of main.rs)
  config.rs        config/token/download dir resolution; host resolution (--host / CRYO_HOST / default)
  client.rs        build the ffsend client
  token_store.rs   OwnerToken + read_map/load/get/save/merge_legacy (moved out of upload.rs)
  secret.rs        resolve(): supply a password, prompting securely only if the file is protected
  commands/
    mod.rs         registers all command modules incl. params
    upload.rs      flags: --downloads --expiry --password --host
    download.rs    flags: --password --out
    exists.rs      reports existence + protection
    info.rs        graceful errors; --password
    delete.rs      uses token_store
    params.rs      NEW: --downloads --expiry --password (owner-token gated)
tests/
  e2e.rs           opt-in #[ignore]d network round-trip
```

**Note on `--host`:** only `upload` consumes it; all other commands derive the host from the share URL. Do not add `--host` to other commands.

---

### Task 1: Add dependencies and fix version string

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/main.rs:10` (clap version attribute)

- [ ] **Step 1: Add dependencies to `Cargo.toml`**

Under `[dependencies]`, add these three lines (keep existing deps):

```toml
anyhow = "1"
directories = "5"
rpassword = "7"
```

- [ ] **Step 2: Make clap read the crate version**

In `src/main.rs`, change the `#[clap(...)]` attribute on `struct Cli` from the hard-coded `version = "0.1"` to derive it from Cargo:

```rust
#[clap(name = "Cryo", version, author = "al", about = "A CLI for secure file sharing using ffsend")]
```

(`version` with no value uses `CARGO_PKG_VERSION`, fixing the 0.1-vs-0.2.1 mismatch.)

- [ ] **Step 3: Verify it builds**

Run: `cargo build`
Expected: compiles (existing code still references `run_menu`/commands — that's fine, we only changed deps + attribute).

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs
git commit -m "build: add anyhow/directories/rpassword, derive clap version from crate"
```

---

### Task 2: `token_store` module (TDD)

Moves `OwnerToken` + persistence out of `upload.rs` into a testable module that takes an explicit path.

**Files:**
- Create: `src/token_store.rs`
- Modify: `src/main.rs` (add `mod token_store;`)

- [ ] **Step 1: Declare the module**

In `src/main.rs`, add near the other `mod` lines:

```rust
mod token_store;
```

- [ ] **Step 2: Write the module with unit tests**

Create `src/token_store.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, Write};
use std::path::Path;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct OwnerToken {
    pub owner_token: String,
}

pub type TokenMap = HashMap<String, OwnerToken>;

/// Read the token map at `path`. Missing or unparseable file => empty map.
pub fn read_map(path: &Path) -> TokenMap {
    match File::open(path) {
        Ok(f) => serde_json::from_reader(BufReader::new(f)).unwrap_or_default(),
        Err(_) => TokenMap::new(),
    }
}

/// Merge legacy entries into `primary` without overwriting existing keys.
pub fn merge_legacy(primary: &mut TokenMap, legacy_path: &Path) {
    for (id, tok) in read_map(legacy_path) {
        primary.entry(id).or_insert(tok);
    }
}

/// Load the token map at `path`, merging any legacy `./owner_token.json`.
pub fn load(path: &Path) -> TokenMap {
    let mut map = read_map(path);
    merge_legacy(&mut map, Path::new("owner_token.json"));
    map
}

/// Look up a single owner token by file id.
pub fn get(path: &Path, file_id: &str) -> Option<String> {
    load(path).get(file_id).map(|t| t.owner_token.clone())
}

/// Insert/update one token and persist the whole map to `path`.
pub fn save(path: &Path, file_id: &str, owner_token: &str) -> std::io::Result<()> {
    let mut map = read_map(path);
    map.insert(
        file_id.to_string(),
        OwnerToken { owner_token: owner_token.to_string() },
    );
    let json = serde_json::to_string_pretty(&map)?;
    let mut f = OpenOptions::new().create(true).write(true).truncate(true).open(path)?;
    f.write_all(json.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn temp_path(name: &str) -> std::path::PathBuf {
        let mut p = env::temp_dir();
        p.push(format!("cryo_test_{}_{}.json", name, std::process::id()));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn save_then_read_round_trips() {
        let path = temp_path("round");
        save(&path, "abc", "tok123").unwrap();
        let map = read_map(&path);
        assert_eq!(map.get("abc").unwrap().owner_token, "tok123");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_twice_keeps_both_ids() {
        let path = temp_path("two");
        save(&path, "id1", "t1").unwrap();
        save(&path, "id2", "t2").unwrap();
        let map = read_map(&path);
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("id1").unwrap().owner_token, "t1");
        assert_eq!(map.get("id2").unwrap().owner_token, "t2");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn merge_legacy_does_not_override_primary() {
        let primary_path = temp_path("primary");
        let legacy_path = temp_path("legacy");
        save(&primary_path, "shared", "new").unwrap();
        save(&legacy_path, "shared", "old").unwrap();
        save(&legacy_path, "legacy_only", "kept").unwrap();

        let mut primary = read_map(&primary_path);
        merge_legacy(&mut primary, &legacy_path);

        assert_eq!(primary.get("shared").unwrap().owner_token, "new");
        assert_eq!(primary.get("legacy_only").unwrap().owner_token, "kept");
        let _ = std::fs::remove_file(&primary_path);
        let _ = std::fs::remove_file(&legacy_path);
    }
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test token_store`
Expected: 3 tests pass. (The binary may still fail to fully build if `upload.rs` already defines `OwnerToken`; if so, that's resolved in Task 5 — run `cargo test --lib token_store` is not applicable for a bin crate, so if the bin doesn't compile yet, temporarily proceed; otherwise tests pass now.)

> If the crate does not compile because `upload.rs` still owns `OwnerToken`, do Task 5 before re-running. The token_store code itself is complete and correct.

- [ ] **Step 4: Commit**

```bash
git add src/token_store.rs src/main.rs
git commit -m "feat: add token_store module with tests"
```

---

### Task 3: `config` module (TDD for host + paths)

**Files:**
- Create: `src/config.rs`
- Modify: `src/main.rs` (add `mod config;`)

- [ ] **Step 1: Declare the module**

In `src/main.rs` add:

```rust
mod config;
```

- [ ] **Step 2: Write the module with tests**

Create `src/config.rs`:

```rust
use anyhow::Result;
use directories::{ProjectDirs, UserDirs};
use std::path::PathBuf;
use url::Url;

pub const DEFAULT_HOST: &str = "https://send.vis.ee/";

/// Choose the raw host string from an explicit flag, then env, then default.
pub fn pick_host(flag: Option<String>, env: Option<String>) -> String {
    flag.or(env).unwrap_or_else(|| DEFAULT_HOST.to_string())
}

/// Resolve the host to a parsed URL (flag > CRYO_HOST env > default).
pub fn resolve_host(flag: Option<String>) -> Result<Url> {
    let raw = pick_host(flag, std::env::var("CRYO_HOST").ok());
    Ok(Url::parse(&raw)?)
}

/// The config directory (created if missing), e.g. ~/.config/cryo.
pub fn config_dir() -> PathBuf {
    let dir = ProjectDirs::from("", "", "cryo")
        .map(|p| p.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Path to the owner-token store inside the config dir.
pub fn token_path() -> PathBuf {
    config_dir().join("owner_token.json")
}

/// Default download directory (OS Downloads dir, else ./downloads).
pub fn default_download_dir() -> PathBuf {
    UserDirs::new()
        .and_then(|u| u.download_dir().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("downloads"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_host_prefers_flag() {
        let h = pick_host(Some("https://flag/".into()), Some("https://env/".into()));
        assert_eq!(h, "https://flag/");
    }

    #[test]
    fn pick_host_falls_back_to_env() {
        let h = pick_host(None, Some("https://env/".into()));
        assert_eq!(h, "https://env/");
    }

    #[test]
    fn pick_host_defaults_when_unset() {
        let h = pick_host(None, None);
        assert_eq!(h, DEFAULT_HOST);
    }

    #[test]
    fn token_path_lives_in_config_dir() {
        let p = token_path();
        assert!(p.ends_with("owner_token.json"));
    }
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test config`
Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/config.rs src/main.rs
git commit -m "feat: add config module (paths + host resolution) with tests"
```

---

### Task 4: `client` and `secret` helper modules

**Files:**
- Create: `src/client.rs`
- Create: `src/secret.rs`
- Modify: `src/main.rs` (add `mod client;` and `mod secret;`)

- [ ] **Step 1: Declare the modules**

In `src/main.rs` add:

```rust
mod client;
mod secret;
```

- [ ] **Step 2: Write `src/client.rs`**

```rust
use ffsend_api::client::{Client, ClientConfigBuilder};

/// Build an ffsend HTTP client with default config.
pub fn build() -> Client {
    ClientConfigBuilder::default()
        .build()
        .expect("Failed to build client config")
        .client(true)
}
```

- [ ] **Step 3: Write `src/secret.rs`**

```rust
use anyhow::Result;
use ffsend_api::action::exists::Exists;
use ffsend_api::client::Client;
use ffsend_api::file::remote_file::RemoteFile;

/// Resolve the password to use for a read.
/// If `provided` is set, use it. Otherwise check whether the file is
/// password-protected and, only if so, prompt securely (no echo).
pub fn resolve(
    file: &RemoteFile,
    client: &Client,
    provided: Option<String>,
) -> Result<Option<String>> {
    if provided.is_some() {
        return Ok(provided);
    }
    let res = Exists::new(file).invoke(client)?;
    if res.requires_password() {
        let pw = rpassword::prompt_password("File password: ")?;
        Ok(Some(pw))
    } else {
        Ok(None)
    }
}
```

- [ ] **Step 4: Verify modules compile**

Run: `cargo build`
Expected: compiles (these modules are self-contained; existing code untouched).

- [ ] **Step 5: Commit**

```bash
git add src/client.rs src/secret.rs src/main.rs
git commit -m "feat: add client builder and password-resolve helpers"
```

---

### Task 5: Refactor `upload` (flags + host + token_store)

Removes `OwnerToken`/`save_token`/`read_tokens_from_file` from `upload.rs` (now in `token_store`) and adds the new flags.

**Files:**
- Modify: `src/commands/upload.rs` (full rewrite)

- [ ] **Step 1: Rewrite `src/commands/upload.rs`**

```rust
use anyhow::Result;
use clap::Args;
use ffsend_api::action::params::ParamsDataBuilder;
use ffsend_api::action::upload::Upload;
use ffsend_api::api::Version;
use std::path::PathBuf;
use url::Url;

use crate::{client, config, token_store};

#[derive(Args)]
pub struct UploadArgs {
    /// Path to the file to upload
    pub file: String,
    /// Maximum number of downloads (default 5)
    #[arg(long)]
    pub downloads: Option<u8>,
    /// Expiry time in seconds (default 3600)
    #[arg(long)]
    pub expiry: Option<usize>,
    /// Protect the file with a password
    #[arg(long)]
    pub password: Option<String>,
    /// Send instance host (default https://send.vis.ee/, or CRYO_HOST)
    #[arg(long)]
    pub host: Option<String>,
}

pub fn upload_file_cmd(args: UploadArgs) -> Result<()> {
    let host = config::resolve_host(args.host)?;
    let client = client::build();

    let params = ParamsDataBuilder::default()
        .download_limit(Some(args.downloads.unwrap_or(5)))
        .expiry_time(Some(args.expiry.unwrap_or(3600)))
        .build()
        .unwrap();

    let upload = Upload::new(
        Version::V3,
        host,
        PathBuf::from(&args.file),
        None,
        args.password,
        Some(params),
    );

    let remote = upload.invoke(&client, None)?;
    let share_url: Url = remote.download_url(true);
    println!("Share URL: {}", share_url);

    if let Some(token) = remote.owner_token() {
        token_store::save(&config::token_path(), remote.id(), token)?;
        println!("Owner token saved");
    } else {
        println!("No owner token returned by server");
    }
    Ok(())
}
```

- [ ] **Step 2: Verify build (will fail on callers until Task 11)**

Run: `cargo build 2>&1 | head -30`
Expected: `upload.rs` itself compiles; remaining errors are only in `main.rs`/`menu` callers and other command files not yet migrated. That is expected — we migrate the rest next.

- [ ] **Step 3: Commit**

```bash
git add src/commands/upload.rs
git commit -m "feat(upload): add --downloads/--expiry/--password/--host, use token_store"
```

---

### Task 6: Refactor `download` (--password, --out, resolved dir)

**Files:**
- Modify: `src/commands/download.rs` (full rewrite)

- [ ] **Step 1: Rewrite `src/commands/download.rs`**

```rust
use anyhow::Result;
use clap::Args;
use ffsend_api::action::download::Download;
use ffsend_api::action::metadata::{Metadata, MetadataResponse};
use ffsend_api::api::Version;
use ffsend_api::file::remote_file::RemoteFile;
use std::path::PathBuf;
use url::Url;

use crate::{client, config, secret};

#[derive(Args)]
pub struct DownloadArgs {
    /// Share URL to download
    pub url: String,
    /// Password, if the file is protected
    #[arg(long)]
    pub password: Option<String>,
    /// Output directory (default: OS Downloads dir)
    #[arg(long)]
    pub out: Option<String>,
}

pub fn download_file_cmd(args: DownloadArgs) -> Result<()> {
    let client = client::build();
    let url = Url::parse(&args.url)?;
    let file = RemoteFile::parse_url(url, None)?;

    let password = secret::resolve(&file, &client, args.password)?;

    let meta: MetadataResponse = Metadata::new(&file, password.clone(), true).invoke(&client)?;
    let file_name = meta.metadata().name().to_string();

    let dir = args.out.map(PathBuf::from).unwrap_or_else(config::default_download_dir);
    std::fs::create_dir_all(&dir)?;
    let target = dir.join(&file_name);

    let download = Download::new(Version::V3, &file, target.clone(), password, true, Some(meta));
    download.invoke(&client, None)?;

    println!("Downloaded to: {}", target.display());
    Ok(())
}
```

- [ ] **Step 2: Verify this file compiles**

Run: `cargo build 2>&1 | grep -A3 'download.rs' | head`
Expected: no errors referencing `download.rs` (remaining errors are other unmigrated files / callers).

- [ ] **Step 3: Commit**

```bash
git add src/commands/download.rs
git commit -m "feat(download): add --password/--out, resolve OS download dir"
```

---

### Task 7: Refactor `info` (graceful errors + --password)

**Files:**
- Modify: `src/commands/info.rs` (full rewrite)

- [ ] **Step 1: Rewrite `src/commands/info.rs`**

```rust
use anyhow::{Context, Result};
use clap::Args;
use ffsend_api::action::info::Info;
use ffsend_api::action::metadata::{Metadata, MetadataResponse};
use ffsend_api::file::remote_file::RemoteFile;
use url::Url;

use crate::{client, config, secret, token_store};

#[derive(Args)]
pub struct InfoArgs {
    /// Share URL to inspect
    pub url: String,
    /// Password, if the file is protected
    #[arg(long)]
    pub password: Option<String>,
}

pub fn info_cmd(args: InfoArgs) -> Result<()> {
    let client = client::build();
    let url = Url::parse(&args.url).context("invalid URL")?;
    let mut file = RemoteFile::parse_url(url, None).context("failed to parse share URL")?;

    let password = secret::resolve(&file, &client, args.password)?;
    let meta: MetadataResponse = Metadata::new(&file, password, true)
        .invoke(&client)
        .context("failed to fetch public metadata")?;

    if let Some(token) = token_store::get(&config::token_path(), file.id()) {
        file.set_owner_token(Some(token));
        let info = Info::new(&file, None)
            .invoke(&client)
            .context("failed to fetch management info")?;
        println!("-- File Info --");
        println!("Download count: {}", info.download_count());
        println!("Download limit: {}", info.download_limit());
        println!("Download left: {}", info.download_left());
    }

    println!("-- File Metadata --");
    println!("File name: {}", meta.metadata().name());
    println!("Content length: {}", meta.size());
    Ok(())
}
```

- [ ] **Step 2: Verify this file compiles**

Run: `cargo build 2>&1 | grep -A3 'info.rs' | head`
Expected: no errors referencing `info.rs`. The `.expect()` panics are gone.

- [ ] **Step 3: Commit**

```bash
git add src/commands/info.rs
git commit -m "feat(info): graceful errors and --password, use token_store"
```

---

### Task 8: Refactor `delete` and `exists`

**Files:**
- Modify: `src/commands/delete.rs` (full rewrite)
- Modify: `src/commands/exists.rs` (full rewrite)

- [ ] **Step 1: Rewrite `src/commands/delete.rs`**

```rust
use anyhow::{anyhow, Result};
use clap::Args;
use ffsend_api::action::delete::Delete;
use ffsend_api::file::remote_file::RemoteFile;
use url::Url;

use crate::{client, config, token_store};

#[derive(Args)]
pub struct DeleteArgs {
    /// Share URL to delete
    pub url: String,
}

pub fn delete_cmd(args: DeleteArgs) -> Result<()> {
    let client = client::build();
    let url = Url::parse(&args.url)?;
    let mut file = RemoteFile::parse_url(url, None)?;

    let token = token_store::get(&config::token_path(), file.id()).ok_or_else(|| {
        anyhow!("no owner token stored for this file; you can only delete files uploaded from this machine")
    })?;
    file.set_owner_token(Some(token));

    Delete::new(&file, None).invoke(&client)?;
    println!("File deleted");
    Ok(())
}
```

- [ ] **Step 2: Rewrite `src/commands/exists.rs`**

```rust
use anyhow::Result;
use clap::Args;
use ffsend_api::action::exists::Exists;
use ffsend_api::file::remote_file::RemoteFile;
use url::Url;

use crate::client;

#[derive(Args)]
pub struct ExistsArgs {
    /// Share URL to check
    pub url: String,
}

pub fn exists_cmd(args: ExistsArgs) -> Result<()> {
    let client = client::build();
    let url = Url::parse(&args.url)?;
    let file = RemoteFile::parse_url(url, None)?;

    let res = Exists::new(&file).invoke(&client)?;
    if res.exists() {
        println!("True, this file exists.");
        if res.requires_password() {
            println!("(password protected)");
        }
    } else {
        println!("False, this file does not exist.");
    }
    Ok(())
}
```

- [ ] **Step 3: Verify these files compile**

Run: `cargo build 2>&1 | grep -A3 -e 'delete.rs' -e 'exists.rs' | head`
Expected: no errors referencing `delete.rs`/`exists.rs`.

- [ ] **Step 4: Commit**

```bash
git add src/commands/delete.rs src/commands/exists.rs
git commit -m "feat(delete,exists): use token_store, anyhow, report protection"
```

---

### Task 9: New `params` command

**Files:**
- Create: `src/commands/params.rs`
- Modify: `src/commands/mod.rs` (register module)

- [ ] **Step 1: Register the module**

Rewrite `src/commands/mod.rs`:

```rust
pub mod upload;
pub mod download;
pub mod exists;
pub mod delete;
pub mod info;
pub mod params;
```

- [ ] **Step 2: Write `src/commands/params.rs`**

```rust
use anyhow::{anyhow, bail, Result};
use clap::Args;
use ffsend_api::action::params::{Params, ParamsData};
use ffsend_api::action::password::Password;
use ffsend_api::file::remote_file::RemoteFile;
use url::Url;

use crate::{client, config, token_store};

#[derive(Args)]
pub struct ParamsArgs {
    /// Share URL of an already-uploaded file you own
    pub url: String,
    /// New maximum number of downloads
    #[arg(long)]
    pub downloads: Option<u8>,
    /// New expiry time in seconds
    #[arg(long)]
    pub expiry: Option<usize>,
    /// New password
    #[arg(long)]
    pub password: Option<String>,
}

pub fn params_cmd(args: ParamsArgs) -> Result<()> {
    if args.downloads.is_none() && args.expiry.is_none() && args.password.is_none() {
        bail!("nothing to change: pass at least one of --downloads, --expiry, --password");
    }

    let client = client::build();
    let url = Url::parse(&args.url)?;
    let mut file = RemoteFile::parse_url(url, None)?;

    let token = token_store::get(&config::token_path(), file.id())
        .ok_or_else(|| anyhow!("no owner token stored for this file"))?;
    file.set_owner_token(Some(token));

    if args.downloads.is_some() || args.expiry.is_some() {
        let params = ParamsData::from(args.downloads, args.expiry);
        Params::new(&file, params, None).invoke(&client)?;
        println!("Updated download limit / expiry");
    }

    if let Some(pw) = args.password.as_deref() {
        Password::new(&file, pw, None).invoke(&client)?;
        println!("Updated password");
    }
    Ok(())
}
```

- [ ] **Step 3: Verify this file compiles**

Run: `cargo build 2>&1 | grep -A3 'params.rs' | head`
Expected: no errors referencing `params.rs` (main.rs dispatch wired in Task 11).

- [ ] **Step 4: Commit**

```bash
git add src/commands/params.rs src/commands/mod.rs
git commit -m "feat(params): add command to change limit/expiry/password after upload"
```

---

### Task 10: Extract `menu` module

**Files:**
- Create: `src/menu.rs`
- Modify: `src/main.rs` (add `mod menu;`, remove old `run_menu`/menu imports in Task 11)

- [ ] **Step 1: Write `src/menu.rs`**

```rust
use std::io::{self, Write};
use terminal_menu::{button, label, menu, mut_menu, run};

use crate::commands;

fn prompt(text: &str) -> String {
    print!("{text}");
    io::stdout().flush().unwrap();
    let mut s = String::new();
    io::stdin().read_line(&mut s).unwrap();
    s.trim().to_string()
}

pub fn run_menu() {
    loop {
        let main_menu = menu(vec![
            label("----------------------------"),
            label("   Cryo, Secure File Sharer  "),
            label("----------------------------"),
            button("Upload"),
            button("Download"),
            button("Exists"),
            button("Information"),
            button("Params"),
            button("Delete"),
            button("Exit"),
        ]);

        run(&main_menu);
        let guard = mut_menu(&main_menu);
        let selection = guard.selected_item_name().to_string();
        drop(guard);

        let result = match selection.as_str() {
            "Upload" => commands::upload::upload_file_cmd(commands::upload::UploadArgs {
                file: prompt("Enter file path to upload: "),
                downloads: None,
                expiry: None,
                password: None,
                host: None,
            }),
            "Download" => commands::download::download_file_cmd(commands::download::DownloadArgs {
                url: prompt("Enter the download link: "),
                password: None,
                out: None,
            }),
            "Exists" => commands::exists::exists_cmd(commands::exists::ExistsArgs {
                url: prompt("Enter link to check if file exists: "),
            }),
            "Information" => commands::info::info_cmd(commands::info::InfoArgs {
                url: prompt("Enter link to check information for: "),
                password: None,
            }),
            "Params" => {
                let url = prompt("Enter link to change params for: ");
                let d = prompt("New download limit (blank to skip): ");
                let e = prompt("New expiry seconds (blank to skip): ");
                commands::params::params_cmd(commands::params::ParamsArgs {
                    url,
                    downloads: d.parse().ok(),
                    expiry: e.parse().ok(),
                    password: None,
                })
            }
            "Delete" => commands::delete::delete_cmd(commands::delete::DeleteArgs {
                url: prompt("Enter link to delete: "),
            }),
            "Exit" => {
                println!("Goodbye!");
                break;
            }
            _ => {
                eprintln!("Invalid selection, try again!");
                Ok(())
            }
        };

        if let Err(e) = result {
            eprintln!("Error: {e:#}");
        }
        prompt("\nPress Enter to continue...");
    }
}
```

Note: download/info auto-prompt for a password via `secret::resolve` only when the file is protected, so the menu passes `password: None`.

- [ ] **Step 2: Declare the module** (wired together with main.rs in Task 11)

Defer adding `mod menu;` to Task 11 where `main.rs` is rewritten.

- [ ] **Step 3: Commit**

```bash
git add src/menu.rs
git commit -m "refactor: extract interactive menu into menu.rs, add Params entry"
```

---

### Task 11: Rewrite `main.rs` (dispatch, modules, exit code)

**Files:**
- Modify: `src/main.rs` (full rewrite)

- [ ] **Step 1: Rewrite `src/main.rs`**

```rust
mod client;
mod commands;
mod config;
mod menu;
mod secret;
mod token_store;

use clap::{Parser, Subcommand};
use std::process::ExitCode;

#[derive(Parser)]
#[clap(name = "Cryo", version, author = "al", about = "A CLI for secure file sharing using ffsend")]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[clap(alias = "u", alias = "up")]
    Upload(commands::upload::UploadArgs),

    #[clap(alias = "d", alias = "down")]
    Download(commands::download::DownloadArgs),

    #[clap(alias = "e")]
    Exists(commands::exists::ExistsArgs),

    #[clap(alias = "del")]
    Delete(commands::delete::DeleteArgs),

    #[clap(alias = "i")]
    Info(commands::info::InfoArgs),

    #[clap(alias = "p")]
    Params(commands::params::ParamsArgs),
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 1 {
        menu::run_menu();
        return ExitCode::SUCCESS;
    }

    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Upload(a) => commands::upload::upload_file_cmd(a),
        Commands::Download(a) => commands::download::download_file_cmd(a),
        Commands::Exists(a) => commands::exists::exists_cmd(a),
        Commands::Delete(a) => commands::delete::delete_cmd(a),
        Commands::Info(a) => commands::info::info_cmd(a),
        Commands::Params(a) => commands::params::params_cmd(a),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e:#}");
            ExitCode::FAILURE
        }
    }
}
```

- [ ] **Step 2: Full build + clippy + tests**

Run: `cargo build`
Expected: clean build, no errors.

Run: `cargo clippy -- -D warnings`
Expected: no warnings. (If clippy flags trivial style in the new modules, fix inline.)

Run: `cargo test`
Expected: the 7 unit tests from Tasks 2–3 pass; no network tests run.

- [ ] **Step 3: Manual smoke test (real network, optional but recommended)**

```bash
echo "hello cryo" > /tmp/cryo_smoke.txt
cargo run -- upload /tmp/cryo_smoke.txt --downloads 2 --expiry 600
# copy the printed Share URL, then:
cargo run -- exists "<SHARE_URL>"
cargo run -- info "<SHARE_URL>"
cargo run -- download "<SHARE_URL>" --out /tmp
cargo run -- delete "<SHARE_URL>"
```
Expected: upload prints a share URL and "Owner token saved"; exists prints True; info prints metadata; download writes `/tmp/cryo_smoke.txt`; delete prints "File deleted".

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire modules, params dispatch, process exit codes"
```

---

### Task 12: README + opt-in integration test

**Files:**
- Modify: `README.md`
- Create: `tests/e2e.rs`

- [ ] **Step 1: Update `README.md`**

Replace the command list so it documents the new flags and the `params` command. Add these sections (keep the intro line and the small copy/paste note):

```markdown
#### Upload a file
```
cargo run -- upload <file_path> [--downloads N] [--expiry SECONDS] [--password PW] [--host URL]
```

#### Download a file
```
cargo run -- download <client_link> [--password PW] [--out DIR]
```

#### Check if a file exists
```
cargo run -- exists <client_link>
```

#### Show file information
```
cargo run -- info <client_link> [--password PW]
```

#### Change a file's parameters (you must own it)
```
cargo run -- params <client_link> [--downloads N] [--expiry SECONDS] [--password PW]
```

#### Delete a file (you must own it)
```
cargo run -- delete <client_link>
```

### Configuration
- Default Send host is `https://send.vis.ee/`. Override per upload with `--host` or set `CRYO_HOST`.
- Owner tokens are stored in your OS config dir (`~/.config/cryo/owner_token.json` on Linux/macOS). A legacy `./owner_token.json` is still read if present.
- Downloads go to your OS Downloads directory unless `--out` is given.

### Install
```
cargo install --path .
```
```

- [ ] **Step 2: Write `tests/e2e.rs`**

```rust
//! Opt-in end-to-end test against a real Send instance.
//! Run with: `cargo test --test e2e -- --ignored`
use std::process::Command;

#[test]
#[ignore = "hits a live Send server; run manually with --ignored"]
fn upload_exists_delete_round_trip() {
    // Create a temp file.
    let file = std::env::temp_dir().join("cryo_e2e.txt");
    std::fs::write(&file, b"cryo e2e payload").unwrap();

    // Upload and capture the share URL from stdout.
    let out = Command::new(env!("CARGO_BIN_EXE_Cryo"))
        .args(["upload", file.to_str().unwrap()])
        .output()
        .expect("run upload");
    assert!(out.status.success(), "upload failed: {:?}", out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let url = stdout
        .lines()
        .find_map(|l| l.strip_prefix("Share URL: "))
        .expect("share URL in output")
        .trim()
        .to_string();

    // Exists should be true.
    let out = Command::new(env!("CARGO_BIN_EXE_Cryo"))
        .args(["exists", &url])
        .output()
        .expect("run exists");
    assert!(String::from_utf8_lossy(&out.stdout).contains("exists"));

    // Delete should succeed (token saved during upload).
    let out = Command::new(env!("CARGO_BIN_EXE_Cryo"))
        .args(["delete", &url])
        .output()
        .expect("run delete");
    assert!(out.status.success(), "delete failed: {:?}", out);
}
```

> The binary env var is `CARGO_BIN_EXE_<name>` where `<name>` is the package name `Cryo` (capitalized, matching `Cargo.toml`).

- [ ] **Step 3: Verify the test is discovered but skipped by default**

Run: `cargo test`
Expected: `e2e` test shows as `ignored`, unit tests pass.

Run (optional, real network): `cargo test --test e2e -- --ignored`
Expected: round-trip passes.

- [ ] **Step 4: Commit**

```bash
git add README.md tests/e2e.rs
git commit -m "docs: document new flags/params; add opt-in e2e test"
```

---

## Self-Review

**Spec coverage:**
- Custom limits & expiry → Task 5 (upload `--downloads`/`--expiry`). ✓
- Password protection → Task 5 (upload `--password`), Task 4/6/7 (`secret::resolve` + download/info `--password`). ✓
- Configurable host → Task 3 (`resolve_host` + `CRYO_HOST`), Task 5 (upload `--host`). Scope refinement (upload-only) noted. ✓
- Change params after upload → Task 9 (`params` via `Params` + `Password`). ✓
- OS config dirs + legacy migration → Task 3 (`config_dir`/`token_path`), Task 2 (`merge_legacy`). ✓
- `client`/`token_store`/`config` extraction → Tasks 2–4. ✓
- Graceful errors (no `info.rs` panics) → Task 7; anyhow + exit code → Task 11. ✓
- Unit tests + opt-in e2e → Tasks 2, 3, 12. ✓
- Version fix + README + packaging → Tasks 1, 12. ✓
- Menu extraction → Task 10. ✓

**Placeholder scan:** none — all steps contain concrete code/commands.

**Type consistency:** `token_store::{read_map,load,get,save,merge_legacy}`, `config::{pick_host,resolve_host,config_dir,token_path,default_download_dir,DEFAULT_HOST}`, `client::build`, `secret::resolve`, and the `*_cmd(args) -> anyhow::Result<()>` signatures are used identically across all consuming tasks. Action signatures (`Upload::new`/`Metadata::new`/`Download::new`/`Params::new`/`Password::new`/`Exists::new`) match the verified `ffsend-api` 0.7.3 source.
