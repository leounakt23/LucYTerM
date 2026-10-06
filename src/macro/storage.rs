//! Macro storage (Prompt 5.2 output path): one RON file per macro under
//! `<config>/macros/`, single-file RON pack bundles for import/export.
//!
//! Format adaptation: RON instead of JSON/YAML (consistent with
//! `config.ron`), bundles instead of ZIP (no zip dependency). Filenames are
//! `{id}.ron`; the id inside the file is authoritative on import.

use std::path::{Path, PathBuf};

use super::{now_secs, Macro};

/// RON-file macro library.
#[derive(Debug, Clone)]
pub struct MacroStore {
    dir: PathBuf,
}

impl MacroStore {
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path_for(&self, id: &uuid::Uuid) -> PathBuf {
        self.dir.join(format!("{id}.ron"))
    }

    /// Load every parseable macro (broken files are reported, not fatal —
    /// one bad file must not hide the whole library).
    pub fn load_all(&self) -> (Vec<Macro>, Vec<String>) {
        let mut macros = Vec::new();
        let mut errors = Vec::new();
        let entries = std::fs::read_dir(&self.dir).map(|read| {
            read.filter_map(Result::ok)
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "ron"))
                .collect::<Vec<_>>()
        });
        match entries {
            Ok(entries) => {
                for entry in entries {
                    match std::fs::read_to_string(entry.path())
                        .map_err(|err| err.to_string())
                        .and_then(|text| {
                            ron::from_str::<Macro>(&text).map_err(|err| err.to_string())
                        }) {
                        Ok(macro_) => macros.push(macro_),
                        Err(reason) => errors.push(format!("{}: {reason}", entry.path().display())),
                    }
                }
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {},
            Err(err) => errors.push(err.to_string()),
        }
        macros.sort_by_key(|a| a.name.to_lowercase());
        (macros, errors)
    }

    /// Save (upsert) one macro, stamping `updated_secs`.
    pub fn save(&self, macro_: &mut Macro) -> Result<PathBuf, String> {
        std::fs::create_dir_all(&self.dir).map_err(|err| err.to_string())?;
        macro_.updated_secs = now_secs();
        let text = ron::ser::to_string_pretty(macro_, ron::ser::PrettyConfig::default())
            .map_err(|err| err.to_string())?;
        let path = self.path_for(&macro_.id);
        std::fs::write(&path, text).map_err(|err| err.to_string())?;
        Ok(path)
    }

    /// Delete one macro file (`true` when a file existed).
    pub fn delete(&self, id: &uuid::Uuid) -> Result<bool, String> {
        match std::fs::remove_file(self.path_for(id)) {
            Ok(()) => Ok(true),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(err) => Err(err.to_string()),
        }
    }

    /// Duplicate under a new id (`"<name> copy"`).
    pub fn duplicate(&self, macro_: &Macro) -> Result<Macro, String> {
        let mut copy = macro_.clone();
        copy.id = uuid::Uuid::new_v4();
        copy.name = format!("{} copy", macro_.name);
        let now = now_secs();
        copy.created_secs = now;
        self.save(&mut copy)?;
        Ok(copy)
    }

    /// Copy built-in library entries that have no file yet (first-run seed).
    /// Returns the number seeded.
    pub fn ensure_seeded(&self, builtins: &[Macro]) -> usize {
        let _ = std::fs::create_dir_all(&self.dir);
        let mut seeded = 0;
        for builtin in builtins {
            let path = self.path_for(&builtin.id);
            if !path.exists() {
                let mut seeded_macro = builtin.clone();
                if self.save(&mut seeded_macro).is_ok() {
                    seeded += 1;
                }
            }
        }
        seeded
    }

    /// Export macros as one RON bundle file.
    pub fn export_bundle(&self, path: &Path, macros: &[Macro]) -> Result<(), String> {
        let text = ron::ser::to_string_pretty(macros, ron::ser::PrettyConfig::default())
            .map_err(|err| err.to_string())?;
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
            }
        }
        std::fs::write(path, text).map_err(|err| err.to_string())
    }

    /// Built-in library (`assets/macros/`, compiled in).
    pub fn builtin_macros() -> Vec<Macro> {
        [
            include_str!("../../assets/macros/system-info.ron"),
            include_str!("../../assets/macros/docker-ps.ron"),
            include_str!("../../assets/macros/disk-usage.ron"),
        ]
        .into_iter()
        .map(|text| ron::from_str::<Macro>(text).expect("built-in macro parses"))
        .collect()
    }

    /// Import a bundle, upserting by id. Returns macros imported.
    pub fn import_bundle(&self, path: &Path) -> Result<Vec<Macro>, String> {
        let text = std::fs::read_to_string(path).map_err(|err| err.to_string())?;
        let mut macros: Vec<Macro> = ron::from_str(&text).map_err(|err| err.to_string())?;
        for macro_ in &mut macros {
            self.save(macro_)?;
        }
        Ok(macros)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(name: &str) -> Macro {
        let mut macro_ = Macro::new(name.into());
        macro_.steps = vec![super::super::MacroStep::SendInput {
            data: "uptime\n".into(),
        }];
        macro_
    }

    #[test]
    fn save_load_delete_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store = MacroStore::new(dir.path());
        let mut macro_ = sample("demo");
        let path = store.save(&mut macro_).unwrap();
        assert!(path.exists());

        let (macros, errors) = store.load_all();
        assert!(errors.is_empty());
        assert_eq!(macros.len(), 1);
        assert_eq!(macros[0].name, "demo");

        assert!(store.delete(&macro_.id).unwrap());
        assert!(!store.delete(&macro_.id).unwrap());
        let (macros, _) = store.load_all();
        assert!(macros.is_empty());
    }

    #[test]
    fn broken_files_reported_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("bad.ron"), "!!! not ron").unwrap();
        let mut macro_ = sample("good");
        MacroStore::new(dir.path()).save(&mut macro_).unwrap();
        let (macros, errors) = MacroStore::new(dir.path()).load_all();
        assert_eq!(macros.len(), 1);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn duplicate_bundle_and_seed() {
        let dir = tempfile::tempdir().unwrap();
        let store = MacroStore::new(dir.path());
        let macro_ = sample("orig");
        let copy = store.duplicate(&macro_).unwrap();
        assert_ne!(copy.id, macro_.id);
        assert!(copy.name.contains("copy"));

        let bundle = dir.path().join("pack.ron");
        store
            .export_bundle(&bundle, std::slice::from_ref(&copy))
            .unwrap();
        store.delete(&copy.id).unwrap();
        let imported = store.import_bundle(&bundle).unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].id, copy.id, "ids survive the bundle");

        let builtin = sample("seeded");
        assert_eq!(store.ensure_seeded(std::slice::from_ref(&builtin)), 1);
        assert_eq!(
            store.ensure_seeded(std::slice::from_ref(&builtin)),
            0,
            "same id is not seeded twice"
        );
    }

    #[test]
    fn builtin_library_parses_and_seeds() {
        let builtins = MacroStore::builtin_macros();
        assert_eq!(builtins.len(), 3);
        assert!(builtins.iter().any(|m| m.name == "System info"));
        assert!(builtins.iter().any(|m| m.name == "List Docker containers"));
        // Docker's own {{.Names}} templates must survive our substitution.
        let docker = builtins
            .iter()
            .find(|m| m.name == "List Docker containers")
            .unwrap();
        assert!(docker
            .steps
            .iter()
            .any(|s| matches!(s, super::super::MacroStep::SendInput { .. })));

        let dir = tempfile::tempdir().unwrap();
        let store = MacroStore::new(dir.path());
        assert_eq!(store.ensure_seeded(&builtins), 3);
        let (macros, errors) = store.load_all();
        assert!(errors.is_empty());
        assert_eq!(macros.len(), 3);
    }
}
