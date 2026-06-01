//! Golden snapshot store with provenance status (spec §5.2).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Unverified,
    Verified,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Status::Unverified => "unverified",
            Status::Verified => "verified",
        }
    }
    fn parse(s: &str) -> Option<Status> {
        match s {
            "unverified" => Some(Status::Unverified),
            "verified" => Some(Status::Verified),
            _ => None,
        }
    }
}

pub struct SnapshotStore {
    dir: PathBuf,
}

impl SnapshotStore {
    pub fn new(dir: impl AsRef<Path>) -> Self {
        SnapshotStore {
            dir: dir.as_ref().to_path_buf(),
        }
    }

    fn snap_path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.xml"))
    }
    fn status_path(&self) -> PathBuf {
        self.dir.join("status.toml")
    }

    fn load_status_map(&self) -> BTreeMap<String, String> {
        match std::fs::read_to_string(self.status_path()) {
            Ok(s) => toml::from_str(&s).unwrap_or_default(),
            Err(_) => BTreeMap::new(),
        }
    }
    fn save_status_map(&self, map: &BTreeMap<String, String>) -> Result<(), String> {
        let s = toml::to_string_pretty(map).map_err(|e| e.to_string())?;
        std::fs::write(self.status_path(), s).map_err(|e| e.to_string())
    }

    pub fn read(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(self.snap_path(name)).ok()
    }

    pub fn status(&self, name: &str) -> Option<Status> {
        self.load_status_map()
            .get(name)
            .and_then(|s| Status::parse(s))
    }

    pub fn write_unverified(&self, name: &str, normalized: &str) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        std::fs::write(self.snap_path(name), normalized).map_err(|e| e.to_string())?;
        let mut map = self.load_status_map();
        map.insert(name.to_string(), Status::Unverified.as_str().to_string());
        self.save_status_map(&map)
    }

    pub fn unverified_count(&self) -> Result<usize, String> {
        Ok(self
            .load_status_map()
            .values()
            .filter(|v| v.as_str() == Status::Unverified.as_str())
            .count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        // Include thread id to avoid collision when tests run in parallel (all share process id).
        p.push(format!(
            "crossref-snap-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn writes_unverified_and_reads_back() {
        let dir = tmp();
        let store = SnapshotStore::new(&dir);
        store.write_unverified("op_x", "<normalized/>").unwrap();
        assert_eq!(store.read("op_x").unwrap(), "<normalized/>");
        assert_eq!(store.status("op_x").unwrap(), Status::Unverified);
    }

    #[test]
    fn missing_snapshot_reads_none() {
        let dir = tmp();
        let store = SnapshotStore::new(&dir);
        assert!(store.read("absent").is_none());
    }

    #[test]
    fn counts_unverified() {
        let dir = tmp();
        let store = SnapshotStore::new(&dir);
        store.write_unverified("a", "<a/>").unwrap();
        store.write_unverified("b", "<b/>").unwrap();
        assert_eq!(store.unverified_count().unwrap(), 2);
    }
}
