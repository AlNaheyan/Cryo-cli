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
