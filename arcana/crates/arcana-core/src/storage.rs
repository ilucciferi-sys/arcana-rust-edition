//! World-scoped player storage. Replacements happen on the same filesystem;
//! the last valid generation remains available if the current file is damaged.
use crate::{Catalog, state::MagicState};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_SAVE_BYTES: u64 = 64 * 1024;
pub struct PlayerStore {
    directory: PathBuf,
}
impl PlayerStore {
    pub fn new(world: &Path) -> Self {
        Self {
            directory: world.join("arcana").join("players"),
        }
    }
    pub fn path(&self, uuid: u128) -> PathBuf {
        self.directory.join(format!("{uuid:032x}.json"))
    }
    fn previous(&self, uuid: u128) -> PathBuf {
        self.path(uuid).with_extension("json.previous")
    }
    fn read(path: &Path, catalog: &Catalog) -> Result<MagicState, String> {
        if fs::metadata(path).map_err(|e| e.to_string())?.len() > MAX_SAVE_BYTES {
            return Err("Arcana save exceeds 64 KiB".into());
        }
        let json = fs::read_to_string(path).map_err(|e| e.to_string())?;
        MagicState::load_json(&json, catalog)
    }
    pub fn load(&self, uuid: u128, catalog: &Catalog) -> Result<Option<MagicState>, String> {
        let current = self.path(uuid);
        let previous = self.previous(uuid);
        if !current.exists() && !previous.exists() {
            return Ok(None);
        }
        Self::read(&current, catalog)
            .or_else(|_| Self::read(&previous, catalog))
            .map(Some)
            .map_err(|e| format!("Cannot load Arcana player {uuid:032x}: {e}"))
    }
    pub fn save(&self, uuid: u128, state: &MagicState, catalog: &Catalog) -> Result<(), String> {
        fs::create_dir_all(&self.directory).map_err(|e| e.to_string())?;
        let json = state.save_json().map_err(|e| e.to_string())?;
        // Validate before touching either generation.
        MagicState::load_json(&json, catalog)?;
        let current = self.path(uuid);
        if current.exists() {
            if Self::read(&current, catalog).is_ok() {
                let old = fs::read(&current).map_err(|e| e.to_string())?;
                replace(&self.previous(uuid), &old)?;
            } else {
                let stamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos();
                fs::copy(
                    &current,
                    current.with_extension(format!("json.corrupt-{stamp}")),
                )
                .map_err(|e| e.to_string())?;
            }
        }
        replace(&current, json.as_bytes())
    }
}
fn replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let pending = path.with_extension(format!(
        "{}.pending",
        path.extension().unwrap_or_default().to_string_lossy()
    ));
    let mut file = fs::File::create(&pending).map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    fs::rename(pending, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::School;
    fn workspace() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("arcana-save-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&p).unwrap();
        p
    }
    fn cleanup(root: PathBuf) {
        let target = root.canonicalize().unwrap();
        let temporary = std::env::temp_dir().canonicalize().unwrap();
        assert_eq!(target.parent(), Some(temporary.as_path()));
        assert!(
            target
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("arcana-save-")
        );
        fs::remove_dir_all(target).unwrap();
    }
    #[test]
    fn players_worlds_and_last_valid_generation_are_isolated() {
        let root = workspace();
        let c = Catalog::bundled().unwrap();
        let store = PlayerStore::new(&root);
        let mut state = MagicState::default();
        state.awaken(School::Fire, &c);
        store.save(1, &state, &c).unwrap();
        let original = state.clone();
        state.set_level(5, &c);
        store.save(1, &state, &c).unwrap();
        assert_eq!(store.load(1, &c).unwrap(), Some(state));
        assert!(store.load(2, &c).unwrap().is_none());
        assert!(
            PlayerStore::new(&root.join("another-world"))
                .load(1, &c)
                .unwrap()
                .is_none()
        );
        fs::write(store.path(1), b"{broken").unwrap();
        assert_eq!(store.load(1, &c).unwrap(), Some(original.clone()));
        store.save(1, &original, &c).unwrap();
        assert_eq!(store.load(1, &c).unwrap(), Some(original));
        assert!(fs::read_dir(root.join("arcana/players")).unwrap().any(|p| {
            p.unwrap()
                .file_name()
                .to_string_lossy()
                .contains("corrupt-")
        }));
        cleanup(root);
    }
    #[test]
    fn unreadable_progress_is_never_interpreted_as_a_new_player() {
        let root = workspace();
        let c = Catalog::bundled().unwrap();
        let store = PlayerStore::new(&root);
        fs::create_dir_all(store.path(1).parent().unwrap()).unwrap();
        fs::write(store.path(1), b"not-json").unwrap();
        assert!(store.load(1, &c).is_err());
        assert_eq!(fs::read(store.path(1)).unwrap(), b"not-json");
        cleanup(root);
    }
}
