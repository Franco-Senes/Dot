//! Rutas XDG (`data`, `config`, `cache`, `runtime`) bajo `dot/` con permisos `0700`,
//! escritura atómica y migración única segura desde el árbol legado `search/`.

use anyhow::{Context, Result};
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const APP_SUBDIR: &str = "dot";
const LEGACY_APP_SUBDIR: &str = "search";
const MIGRATION_MARKER_FILE: &str = ".migrated-from-search";
static ATOMIC_SEQ: AtomicU64 = AtomicU64::new(1);

/// Resultado de la comprobación/migración única desde `.../search` hacia `.../dot`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationOutcome {
    /// El archivo marcador `.migrated-from-search` ya existía; no se volvió a ejecutar.
    AlreadyMigrated,
    /// No existían directorios heredados `.../search`; se escribió el marcador.
    NothingToMigrate,
    /// Se movieron con éxito los directorios `.../search` indicados y se escribió el marcador.
    Migrated { moved_dirs: Vec<PathBuf> },
    /// Algún directorio no pudo moverse (p. ej. permisos o distinto dispositivo);
    /// los datos antiguos en `.../search` quedaron intactos.
    Failed {
        moved_dirs: Vec<PathBuf>,
        errors: Vec<String>,
    },
}

/// Conjunto de directorios XDG del navegador bajo `dot/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DotPaths {
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub runtime_dir: PathBuf,
}

impl DotPaths {
    /// Resuelve las rutas XDG estándar del entorno actual (con soporte de aislamiento
    /// mediante `DOT_PROFILE_DIR` o respaldo obsoleto `SEARCH_PROFILE_DIR` para pruebas).
    pub fn from_env() -> Result<Self> {
        if let Some(override_root) = crate::env::var("DOT_PROFILE_DIR", "SEARCH_PROFILE_DIR") {
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
            std::env::temp_dir().join(format!("dot-runtime-{uid}"))
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

    /// Ruta del archivo marcador que garantiza que la migración desde `search/` solo ocurra una vez.
    pub fn migration_marker_file(&self) -> PathBuf {
        self.data_dir.join(MIGRATION_MARKER_FILE)
    }

    /// Migra una sola vez los directorios persistentes `.../search` hacia `.../dot` usando
    /// `fs::rename` (mover, nunca copiar). Si falla algún renombrado, deja intactos los datos
    /// originales en `.../search` y registra una advertencia.
    pub fn migrate_from_search_if_needed(&self) -> MigrationOutcome {
        let marker = self.migration_marker_file();
        if marker.exists() {
            return MigrationOutcome::AlreadyMigrated;
        }

        let mut moved_dirs = Vec::new();
        let mut errors = Vec::new();
        let mut saw_legacy = false;

        for new_dir in [&self.data_dir, &self.config_dir, &self.cache_dir] {
            if new_dir.file_name() != Some(OsStr::new(APP_SUBDIR)) {
                continue;
            }
            let Some(parent) = new_dir.parent() else {
                continue;
            };
            let old_dir = parent.join(LEGACY_APP_SUBDIR);
            if !old_dir.is_dir() {
                continue;
            }
            saw_legacy = true;

            if new_dir.exists() {
                let msg = format!(
                    "No se migró '{}' porque '{}' ya existe; se conservan los datos antiguos intactos",
                    old_dir.display(),
                    new_dir.display()
                );
                crate::warn!("{msg}");
                errors.push(msg);
                continue;
            }

            match fs::rename(&old_dir, new_dir) {
                Ok(()) => {
                    crate::info!(
                        "Directorio XDG migrado (movido): '{}' -> '{}'",
                        old_dir.display(),
                        new_dir.display()
                    );
                    moved_dirs.push(new_dir.clone());
                }
                Err(err) => {
                    let msg = format!(
                        "Error al migrar '{}' -> '{}': {err}. Los datos antiguos en '{}' se han dejado intactos.",
                        old_dir.display(),
                        new_dir.display(),
                        old_dir.display()
                    );
                    crate::warn!("{msg}");
                    errors.push(msg);
                }
            }
        }

        if !errors.is_empty() {
            return MigrationOutcome::Failed { moved_dirs, errors };
        }

        if let Err(err) = create_dir_0700(&self.data_dir)
            .and_then(|()| write_atomic(&marker, b"migrated-from-search\n"))
        {
            crate::warn!(
                "No se pudo escribir el marcador de migración '{}': {err:#}",
                marker.display()
            );
        }

        if saw_legacy {
            MigrationOutcome::Migrated { moved_dirs }
        } else {
            MigrationOutcome::NothingToMigrate
        }
    }

    /// Ejecuta la migración única desde `search/` (si procede) y crea los cuatro directorios XDG
    /// con permisos `0700`.
    pub fn ensure_dirs(&self) -> Result<()> {
        let _ = self.migrate_from_search_if_needed();
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

    pub fn history_file(&self) -> PathBuf {
        self.data_dir.join("history.json")
    }

    pub fn bookmarks_file(&self) -> PathBuf {
        self.data_dir.join("bookmarks.json")
    }

    pub fn downloads_file(&self) -> PathBuf {
        self.data_dir.join("downloads.json")
    }

    pub fn pin_folders_file(&self) -> PathBuf {
        self.data_dir.join("pin-folders.json")
    }

    pub fn curtain_file(&self) -> PathBuf {
        self.data_dir.join("curtain.json")
    }

    pub fn boosts_file(&self) -> PathBuf {
        self.data_dir.join("boosts.json")
    }

    pub fn extensions_dir(&self) -> PathBuf {
        self.data_dir.join("extensions")
    }

    pub fn filters_cache_dir(&self) -> PathBuf {
        self.cache_dir.join("filters")
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
        std::env::temp_dir().join(format!("dot-core-paths-{label}-{pid}-{seq}"))
    }

    #[test]
    fn ensure_dirs_creates_all_xdg_dirs_with_0700() {
        let root = unique_temp_dir("dirs");
        let paths = DotPaths::with_base(&root);
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
        assert!(paths.migration_marker_file().is_file());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn write_atomic_persists_and_replaces_cleanly() {
        let root = unique_temp_dir("atomic");
        let paths = DotPaths::with_base(&root);
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
        let paths = DotPaths::with_base(&root);
        paths.ensure_dirs().expect("ensure_dirs");

        let broken = paths.data_dir.join("session.json");
        write_atomic(&broken, b"{corrupt-json").expect("write");
        let bak = backup_corrupt_file(&broken).expect("backup");
        assert_eq!(bak, paths.data_dir.join("session.json.bak"));
        assert_eq!(fs::read(&bak).expect("read bak"), b"{corrupt-json");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn migration_moves_legacy_search_dirs_once_and_writes_marker() {
        let root = unique_temp_dir("migrate-ok");
        let old_data = root.join("data").join("search");
        let old_config = root.join("config").join("search");
        let old_cache = root.join("cache").join("search");
        fs::create_dir_all(&old_data).expect("create old data");
        fs::create_dir_all(&old_config).expect("create old config");
        fs::create_dir_all(&old_cache).expect("create old cache");
        fs::write(old_data.join("bookmarks.json"), b"[\"bm\"]").expect("write bookmarks");
        fs::write(old_config.join("settings.json"), b"{\"look\":\"dark\"}")
            .expect("write settings");
        fs::write(old_cache.join("filter.bin"), b"cache").expect("write cache");

        let paths = DotPaths::with_base(&root);
        let outcome = paths.migrate_from_search_if_needed();
        assert_eq!(
            outcome,
            MigrationOutcome::Migrated {
                moved_dirs: vec![
                    paths.data_dir.clone(),
                    paths.config_dir.clone(),
                    paths.cache_dir.clone(),
                ]
            }
        );

        // Los directorios antiguos se movieron (no se copiaron).
        assert!(!old_data.exists());
        assert!(!old_config.exists());
        assert!(!old_cache.exists());

        // Los datos existen intactos bajo `dot/`.
        assert_eq!(
            fs::read_to_string(paths.bookmarks_file()).expect("read bookmarks"),
            "[\"bm\"]"
        );
        assert_eq!(
            fs::read_to_string(paths.settings_file()).expect("read settings"),
            "{\"look\":\"dark\"}"
        );
        assert_eq!(
            fs::read_to_string(paths.cache_dir.join("filter.bin")).expect("read cache"),
            "cache"
        );
        assert!(paths.migration_marker_file().is_file());

        // Una segunda llamada nunca vuelve a ejecutarse aunque reaparezca `search/`.
        fs::create_dir_all(&old_config).expect("recreate legacy config");
        fs::write(old_config.join("settings.json"), b"{\"look\":\"light\"}").expect("write");
        assert_eq!(
            paths.migrate_from_search_if_needed(),
            MigrationOutcome::AlreadyMigrated
        );
        assert!(old_config.exists());
        assert_eq!(
            fs::read_to_string(paths.settings_file()).expect("still dark"),
            "{\"look\":\"dark\"}"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn migration_failure_leaves_legacy_search_data_untouched() {
        let root = unique_temp_dir("migrate-fail");
        let old_config = root.join("config").join("search");
        let new_config = root.join("config").join("dot");
        fs::create_dir_all(&old_config).expect("create old config");
        fs::create_dir_all(&new_config).expect("create existing new config");
        fs::write(old_config.join("settings.json"), b"{\"legacy\":true}")
            .expect("write legacy settings");
        fs::write(new_config.join("settings.json"), b"{\"current\":true}")
            .expect("write current settings");

        let paths = DotPaths::with_base(&root);
        let outcome = paths.migrate_from_search_if_needed();
        assert!(matches!(outcome, MigrationOutcome::Failed { .. }));

        // Los datos antiguos y los actuales deben permanecer completamente intactos.
        assert_eq!(
            fs::read_to_string(old_config.join("settings.json")).expect("legacy untouched"),
            "{\"legacy\":true}"
        );
        assert_eq!(
            fs::read_to_string(new_config.join("settings.json")).expect("current untouched"),
            "{\"current\":true}"
        );
        assert!(!paths.migration_marker_file().exists());

        let _ = fs::remove_dir_all(&root);
    }
}
