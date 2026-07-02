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
