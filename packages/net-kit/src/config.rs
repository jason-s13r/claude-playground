//! Reading and writing a TOML config file.
//!
//! A missing file is the default configuration, not an error -- a first run has
//! no config and should still work. Anything written lands owner-only, because
//! a config may name a `password_command`.

use std::path::Path;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{Error, Result};
use crate::paths::restrict;

pub fn load_toml<T: DeserializeOwned + Default>(file: &Path) -> Result<T> {
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(T::default()),
        Err(e) => return Err(Error::io(format!("reading {}", file.display()), e)),
    };
    toml::from_str(&text).map_err(|e| Error::Toml {
        context: format!("reading {}", file.display()),
        detail: e.to_string(),
    })
}

pub fn save_toml<T: Serialize>(file: &Path, value: &T) -> Result<()> {
    let text = toml::to_string_pretty(value).map_err(|e| Error::Toml {
        context: format!("writing {}", file.display()),
        detail: e.to_string(),
    })?;
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::io(format!("creating {}", parent.display()), e))?;
    }
    std::fs::write(file, text).map_err(|e| Error::io(format!("writing {}", file.display()), e))?;
    restrict(file);
    Ok(())
}

/// Read a JSON state file, treating "absent" and "unreadable" alike.
///
/// State files are caches: a corrupt one costs this run a re-fetch, which is
/// strictly better than refusing to run at all.
pub fn load_json_cache<T: DeserializeOwned>(file: &Path) -> Option<T> {
    let text = std::fs::read_to_string(file).ok()?;
    serde_json::from_str(&text).ok()
}

/// Write a JSON state file, owner-only. Best effort for the same reason.
pub fn save_json_cache<T: Serialize>(file: &Path, value: &T) {
    let Ok(text) = serde_json::to_string(value) else {
        return;
    };
    if let Some(parent) = file.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    if std::fs::write(file, text).is_ok() {
        restrict(file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    struct Example {
        #[serde(skip_serializing_if = "Option::is_none")]
        store_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        retailer: Option<String>,
    }

    #[test]
    fn a_missing_file_is_the_default_not_an_error() {
        let dir = tempfile::TempDir::new().unwrap();
        let loaded: Example = load_toml(&dir.path().join("absent.toml")).unwrap();
        assert_eq!(loaded, Example::default());
    }

    #[test]
    fn round_trips_and_omits_empty_fields() {
        let dir = tempfile::TempDir::new().unwrap();
        let file = dir.path().join("nested/config.toml");
        let value = Example {
            store_id: Some("4123".into()),
            retailer: None,
        };
        save_toml(&file, &value).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("store_id"));
        assert!(!text.contains("retailer"), "empty fields are not written");
        assert_eq!(load_toml::<Example>(&file).unwrap(), value);
    }

    #[test]
    fn malformed_toml_names_the_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let file = dir.path().join("config.toml");
        std::fs::write(&file, "this is not = = toml").unwrap();
        let err = load_toml::<Example>(&file).unwrap_err();
        assert!(err.to_string().contains("config.toml"), "{err}");
    }

    #[test]
    fn a_corrupt_cache_reads_as_absent() {
        let dir = tempfile::TempDir::new().unwrap();
        let file = dir.path().join("token.json");
        std::fs::write(&file, "{ truncated").unwrap();
        assert!(load_json_cache::<Example>(&file).is_none());
    }

    #[test]
    fn cache_round_trips() {
        let dir = tempfile::TempDir::new().unwrap();
        let file = dir.path().join("sub/token.json");
        save_json_cache(
            &file,
            &Example {
                store_id: Some("9".into()),
                retailer: None,
            },
        );
        let back: Example = load_json_cache(&file).unwrap();
        assert_eq!(back.store_id.as_deref(), Some("9"));
    }
}
