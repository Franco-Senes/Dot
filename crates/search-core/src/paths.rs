//! Rutas XDG (`data`, `config`, `cache`, `runtime`) con permisos `0700` y escritura atómica.

use anyhow::{Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const APP_SUBDIR: &str = "search";
static ATOMIC_SEQ: AtomicU64 = AtomicU64::new(1);

/// Conjunto de directorios XDG del navegador bajo `search/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchPaths {
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub runtime_dir: PathBuf,
}

impl SearchPaths {
    /// Resuelve las rutas XDG estándar del entorno actual (con soporte de aislamiento
    /// mediante `SEARCH_PROFILE_DIR` para pruebas).
    pub fn from_env() -> Result<Self> {
        if let Ok(override_root) = std::env::var("SEARCH_PROFILE_DIR") {
            let trimmed = override_root.trim();
            if !trimmed.is_empty() {
                return Ok(Self::with_base(Path::new(trimmed)));
            }
        }

        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())
            .context("La variable de entorno HOME no está definida")?;

        let data_base =
            env_dir("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local").join("share"));
        let config_base = env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"));
        let cache_base = env_dir("XDG_CACHE_HOME").unwrap_or_else(|| home.join(".cache"));
        let runtime_base = env_dir("XDG_RUNTIME_DIR").unwrap_or_else(|| {
            #[cfg(unix)]
            let uid = unsafe { libc::geteuid() };
            #[cfg(not(unix))]
            let uid = 0u32;
            std::env::temp_dir().join(format!("search-runtime-{uid}"))
        });

        Ok(Self {
            data_dir: data_base.join(APP_SUBDIR),
            config_dir: config_base.join(APP_SUBDIR),
            cache_dir: cache_base.join(APP_SUBDIR),
            runtime_dir: runtime_base.join(APP_SUBDIR),
        })
    }

    /// Crea un árbol aislado de rutas (útil para tests unitarios y headless).
    pub fn with_base(base: &Path) -> Self {
        Self {
            data_dir: base.join("data").join(APP_SUBDIR),
            config_dir: base.join("config").join(APP_SUBDIR),
            cache_dir: base.join("cache").join(APP_SUBDIR),
            runtime_dir: base.join("runtime").join(APP_SUBDIR),
        }
    }

    /// Crea los cuatro directorios XDG con permisos `0700`.
    pub fn ensure_dirs(&self) -> Result<()> {
        for dir in [
            &self.data_dir,
            &self.config_dir,
            &self.cache_dir,
            &self.runtime_dir,
        ] {
            create_dir_0700(dir)?;
        }
        Ok(())
    }

    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    pub fn session_file(&self) -> PathBuf {
        self.data_dir.join("session.json")
    }

    pub fn cef_cache_dir(&self) -> PathBuf {
        self.cache_dir.join("cef")
    }

    pub fn bench_socket(&self) -> PathBuf {
        self.runtime_dir.join("bench.sock")
    }
}

fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

/// Crea un directorio (y sus padres) aplicando permisos `0700` al directorio hoja.
pub fn create_dir_0700(path: &Path) -> Result<()> {
    fs::create_dir_all(path)
        .with_context(|| format!("No se pudo crear el directorio '{}'", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).with_context(|| {
            format!("No se pudieron fijar permisos 0700 en '{}'", path.display())
        })?;
    }
    Ok(())
}

/// Escribe `bytes` de forma atómica creando un archivo temporal con permisos `0600`
/// en el mismo directorio, sincronizando a disco (`sync_all`) y renombrando sobre `path`.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.exists() {
        create_dir_0700(parent)?;
    }

    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("data");
    let pid = std::process::id();
    let seq = ATOMIC_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp_path = parent.join(format!(".{file_name}.{pid}.{nanos}.{seq}.tmp"));

    let write_res = (|| -> Result<()> {
        let mut opts = OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut file = opts.open(&tmp_path).with_context(|| {
            format!(
                "No se pudo crear el archivo temporal '{}'",
                tmp_path.display()
            )
        })?;
        file.write_all(bytes).with_context(|| {
            format!(
                "Error al escribir en archivo temporal '{}'",
                tmp_path.display()
            )
        })?;
        file.sync_all().with_context(|| {
            format!(
                "Error al sincronizar archivo temporal '{}'",
                tmp_path.display()
            )
        })?;
        drop(file);
        fs::rename(&tmp_path, path).with_context(|| {
            format!(
                "No se pudo renombrar '{}' sobre '{}'",
                tmp_path.display(),
                path.display()
            )
        })?;
        Ok(())
    })();

    if write_res.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }
    write_res
}

/// Respalda un archivo corrupto en `<ruta>.bak` para no perder datos antes de recrearlo limpio.
pub fn backup_corrupt_file(path: &Path) -> Result<PathBuf> {
    let mut bak_os = path.as_os_str().to_os_string();
    bak_os.push(".bak");
    let bak_path = PathBuf::from(bak_os);
    if path.exists() {
        fs::copy(path, &bak_path).with_context(|| {
            format!(
                "No se pudo guardar copia de respaldo '{}' desde '{}'",
                bak_path.display(),
                path.display()
            )
        })?;
    }
    Ok(bak_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn unique_temp_dir(label: &str) -> PathBuf {
        let seq = ATOMIC_SEQ.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        std::env::temp_dir().join(format!("search-core-paths-{label}-{pid}-{seq}"))
    }

    #[test]
    fn ensure_dirs_creates_all_xdg_dirs_with_0700() {
        let root = unique_temp_dir("dirs");
        let paths = SearchPaths::with_base(&root);
        paths.ensure_dirs().expect("ensure_dirs debe funcionar");

        for dir in [
            &paths.data_dir,
            &paths.config_dir,
            &paths.cache_dir,
            &paths.runtime_dir,
        ] {
            let meta = fs::metadata(dir).expect("directorio creado");
            assert!(meta.is_dir());
            assert_eq!(meta.permissions().mode() & 0o777, 0o700);
        }

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn write_atomic_persists_and_replaces_cleanly() {
        let root = unique_temp_dir("atomic");
        let paths = SearchPaths::with_base(&root);
        paths.ensure_dirs().expect("ensure_dirs");

        let target = paths.config_dir.join("settings.json");
        write_atomic(&target, br#"{"look":"dark"}"#).expect("primera escritura");
        assert_eq!(
            fs::read_to_string(&target).expect("lectura"),
            r#"{"look":"dark"}"#
        );

        write_atomic(&target, br#"{"look":"light"}"#).expect("segunda escritura");
        assert_eq!(
            fs::read_to_string(&target).expect("lectura"),
            r#"{"look":"light"}"#
        );

        let meta = fs::metadata(&target).expect("metadata");
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);

        // Ningún temporal residual en el directorio.
        let count = fs::read_dir(&paths.config_dir).expect("read_dir").count();
        assert_eq!(count, 1);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn backup_corrupt_file_creates_bak() {
        let root = unique_temp_dir("bak");
        let paths = SearchPaths::with_base(&root);
        paths.ensure_dirs().expect("ensure_dirs");

        let broken = paths.data_dir.join("session.json");
        write_atomic(&broken, b"{corrupt-json").expect("write");
        let bak = backup_corrupt_file(&broken).expect("backup");
        assert_eq!(bak, paths.data_dir.join("session.json.bak"));
        assert_eq!(fs::read(&bak).expect("read bak"), b"{corrupt-json");

        let _ = fs::remove_dir_all(&root);
    }
}
