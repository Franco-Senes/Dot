//! Almacenamiento local y manifiesto `installed.json` de extensiones CRX3 en
//! `$XDG_DATA_HOME/search/extensions` (Fase 9 — Paso 2B).
//!
//! Nota sobre CEF 156: el renderizado windowless OSR utiliza siempre
//! `CEF_RUNTIME_STYLE_ALLOY`, donde no existe la antigua API `CefExtension` ni el
//! subsistema `//chrome` de extensiones MV3 sin interfaz de Chrome. Por ello este
//! almacén gestiona el ciclo de vida seguro en disco (verificación criptográfica
//! CRX3 vía `crate::crx`, extracción atómica y `installed.json`) detrás del
//! ajuste experimental desactivado por defecto `extensions_enabled`.

use crate::crx::{self, ExtractLimits};
use crate::paths;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const INSTALLED_FILE: &str = "installed.json";

/// Subconjunto relevante de `manifest.json` dentro de una extensión desempaquetada.
#[derive(Debug, Clone, Deserialize)]
pub struct ExtensionManifest {
    #[serde(default = "default_manifest_version")]
    pub manifest_version: u32,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub host_permissions: Vec<String>,
}

fn default_manifest_version() -> u32 {
    3
}

/// Metadatos persistidos de una extensión instalada en `$XDG_DATA_HOME/search/extensions`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledExtension {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_manifest_version")]
    pub manifest_version: u32,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub enabled: bool,
    pub install_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct InstalledFile {
    #[serde(default = "default_schema_version")]
    version: u32,
    #[serde(default)]
    extensions: Vec<InstalledExtension>,
}

fn default_schema_version() -> u32 {
    1
}

/// Almacén atómico de extensiones instaladas bajo `$XDG_DATA_HOME/search/extensions`.
#[derive(Debug, Clone)]
pub struct ExtensionStore {
    root_dir: PathBuf,
    extensions: Vec<InstalledExtension>,
}

impl ExtensionStore {
    /// Carga `installed.json` desde `root_dir`, recuperando `.bak` si el JSON está corrupto.
    pub fn load(root_dir: impl Into<PathBuf>) -> Result<Self> {
        let root_dir = root_dir.into();
        let manifest_path = root_dir.join(INSTALLED_FILE);
        if !manifest_path.exists() {
            return Ok(Self {
                root_dir,
                extensions: Vec::new(),
            });
        }

        let raw = fs::read_to_string(&manifest_path)
            .with_context(|| format!("No se pudo leer '{}'", manifest_path.display()))?;
        match serde_json::from_str::<InstalledFile>(&raw) {
            Ok(parsed) => Ok(Self {
                root_dir,
                extensions: parsed.extensions,
            }),
            Err(err) => {
                crate::warn!(
                    "JSON corrupto en '{}': {}. Guardando respaldo .bak.",
                    manifest_path.display(),
                    err
                );
                if let Ok(bak) = paths::backup_corrupt_file(&manifest_path) {
                    if let Ok(bak_raw) = fs::read_to_string(&bak) {
                        if let Ok(parsed) = serde_json::from_str::<InstalledFile>(&bak_raw) {
                            return Ok(Self {
                                root_dir,
                                extensions: parsed.extensions,
                            });
                        }
                    }
                }
                Ok(Self {
                    root_dir,
                    extensions: Vec::new(),
                })
            }
        }
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn list(&self) -> &[InstalledExtension] {
        &self.extensions
    }

    /// Persiste `installed.json` de forma atómica con permisos `0700`.
    pub fn save(&self) -> Result<()> {
        paths::create_dir_0700(&self.root_dir)?;
        let manifest_path = self.root_dir.join(INSTALLED_FILE);
        let payload = InstalledFile {
            version: default_schema_version(),
            extensions: self.extensions.clone(),
        };
        let json = serde_json::to_string_pretty(&payload)?;
        paths::write_atomic(&manifest_path, json.as_bytes())
    }

    /// Verifica un paquete CRX3 (firma + ID + extracción ZIP segura) y registra la extensión.
    pub fn install_from_crx(
        &mut self,
        expected_id: Option<&str>,
        crx_bytes: &[u8],
    ) -> Result<InstalledExtension> {
        let parsed = crx::parse_crx3(crx_bytes)?;
        let id = match expected_id {
            Some(exp) => {
                if !crx::is_valid_extension_id(exp) {
                    bail!("Identificador de extensión inválido: '{exp}'");
                }
                if exp != parsed.crx_id {
                    bail!(
                        "El ID del paquete CRX3 ({}) no coincide con el esperado ({exp})",
                        parsed.crx_id
                    );
                }
                exp.to_string()
            }
            None => parsed.crx_id.clone(),
        };

        paths::create_dir_0700(&self.root_dir)?;
        let dest = self.root_dir.join(&id);
        crx::verify_and_extract_crx3(crx_bytes, &id, &dest, ExtractLimits::default())?;

        let manifest = match read_extension_manifest(&dest) {
            Ok(m) => m,
            Err(err) => {
                let _ = fs::remove_dir_all(&dest);
                return Err(err);
            }
        };

        let mut permissions = manifest.permissions;
        for host in manifest.host_permissions {
            if !permissions.contains(&host) {
                permissions.push(host);
            }
        }

        let entry = InstalledExtension {
            id: id.clone(),
            name: manifest.name,
            version: manifest.version,
            description: manifest.description,
            manifest_version: manifest.manifest_version,
            permissions,
            enabled: false,
            install_dir: dest,
        };

        if let Some(existing) = self.extensions.iter_mut().find(|e| e.id == id) {
            *existing = entry.clone();
        } else {
            self.extensions.push(entry.clone());
        }
        self.save()?;
        Ok(entry)
    }

    /// Activa o desactiva una extensión registrada por ID.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<bool> {
        let Some(ext) = self.extensions.iter_mut().find(|e| e.id == id) else {
            return Ok(false);
        };
        ext.enabled = enabled;
        self.save()?;
        Ok(true)
    }

    /// Desinstala una extensión por ID y elimina su directorio en `$XDG_DATA_HOME/search/extensions`.
    pub fn remove(&mut self, id: &str) -> Result<bool> {
        let Some(idx) = self.extensions.iter().position(|e| e.id == id) else {
            return Ok(false);
        };
        let removed = self.extensions.remove(idx);
        if crx::is_valid_extension_id(&removed.id) {
            let expected_dir = self.root_dir.join(&removed.id);
            if expected_dir.exists() {
                fs::remove_dir_all(&expected_dir)
                    .with_context(|| format!("No se pudo eliminar '{}'", expected_dir.display()))?;
            }
        }
        self.save()?;
        Ok(true)
    }
}

fn read_extension_manifest(dir: &Path) -> Result<ExtensionManifest> {
    let manifest_path = dir.join("manifest.json");
    let raw = fs::read_to_string(&manifest_path)
        .with_context(|| format!("No se pudo leer '{}'", manifest_path.display()))?;
    let manifest: ExtensionManifest = serde_json::from_str(&raw)
        .with_context(|| format!("manifest.json inválido en '{}'", manifest_path.display()))?;
    if manifest.name.trim().is_empty() || manifest.version.trim().is_empty() {
        bail!("manifest.json debe definir 'name' y 'version' no vacíos");
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_ext_root(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "search-ext-test-{label}-{}-{nanos}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn store_roundtrip_enable_remove_and_corrupt_recovery() {
        let root = temp_ext_root("roundtrip");
        let mut store = ExtensionStore::load(&root).expect("load empty");
        assert!(store.list().is_empty());

        let fake_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let ext_dir = root.join(fake_id);
        fs::create_dir_all(&ext_dir).unwrap();
        store.extensions.push(InstalledExtension {
            id: fake_id.to_string(),
            name: "Demo Ext".to_string(),
            version: "1.0.0".to_string(),
            description: "Prueba".to_string(),
            manifest_version: 3,
            permissions: vec!["storage".to_string()],
            enabled: false,
            install_dir: ext_dir.clone(),
        });
        store.save().unwrap();

        let mut reloaded = ExtensionStore::load(&root).unwrap();
        assert_eq!(reloaded.list().len(), 1);
        assert!(!reloaded.list()[0].enabled);

        assert!(reloaded.set_enabled(fake_id, true).unwrap());
        assert!(reloaded.list()[0].enabled);

        // Corromper installed.json y verificar recuperación sin pánico
        let manifest_path = root.join(INSTALLED_FILE);
        fs::write(&manifest_path, "{corrupt json").unwrap();
        let recovered = ExtensionStore::load(&root).unwrap();
        assert!(recovered.list().is_empty());

        assert!(reloaded.remove(fake_id).unwrap());
        assert!(!ext_dir.exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn invalid_crx_is_rejected() {
        let root = temp_ext_root("invalid-crx");
        let mut store = ExtensionStore::load(&root).unwrap();
        let err = store.install_from_crx(None, b"not-a-crx").unwrap_err();
        assert!(err.to_string().contains("CRX3") || err.to_string().contains("crx"));
        let _ = fs::remove_dir_all(&root);
    }
}
