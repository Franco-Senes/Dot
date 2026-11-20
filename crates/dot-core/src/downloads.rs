//! Modelo puro de descargas (`$XDG_DATA_HOME/search/downloads.json`), renombrado seguro sin
//! sobrescribir (`archivo (1).pdf`) y formateo legible de tamaño y velocidad (`Loot.swift`).

use crate::address::sanitize_url_for_ui;
use crate::paths;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Máximo de descargas completadas recordadas en `downloads.json`.
pub const MAX_DOWNLOAD_HISTORY: usize = 200;

/// Registro persistente de una descarga completada (coincide con `PORTING.md` §4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DownloadRecord {
    pub id: String,
    pub name: String,
    pub path: String,
    pub url: String,
    pub bytes: u64,
    pub finished: f64,
}

impl DownloadRecord {
    pub fn file_exists(&self) -> bool {
        Path::new(&self.path).exists()
    }
}

/// Almacén persistente del historial de descargas completadas (solo ventanas no privadas).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DownloadHistoryStore {
    records: Vec<DownloadRecord>,
}

impl DownloadHistoryStore {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    pub fn records(&self) -> &[DownloadRecord] {
        &self.records
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Carga `downloads.json`. Ante JSON corrupto crea `<ruta>.bak`, emite `warn!` y arranca limpio.
    pub fn load(path: &Path) -> Self {
        if !path.exists() {
            return Self::new();
        }
        let raw = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(err) => {
                crate::warn!(
                    "No se pudo leer '{}': {err}. Arrancando lista de descargas vacía.",
                    path.display()
                );
                return Self::new();
            }
        };
        match serde_json::from_str::<Vec<DownloadRecord>>(&raw) {
            Ok(mut records) => {
                for r in &mut records {
                    r.url = sanitize_url_for_ui(&r.url);
                }
                records.truncate(MAX_DOWNLOAD_HISTORY);
                Self { records }
            }
            Err(err) => {
                crate::warn!(
                    "JSON corrupto en descargas '{}': {err}. Guardando respaldo .bak.",
                    path.display()
                );
                if let Err(bak_err) = paths::backup_corrupt_file(path) {
                    crate::warn!("No se pudo respaldar '{}': {bak_err}", path.display());
                }
                Self::new()
            }
        }
    }

    /// Guarda de forma atómica `downloads.json`.
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut json = serde_json::to_vec_pretty(&self.records)?;
        json.push(b'\n');
        paths::write_atomic(path, &json)
    }

    /// Inserta al frente una descarga completada (deduplicando por ruta).
    pub fn add(&mut self, mut record: DownloadRecord) {
        record.url = sanitize_url_for_ui(&record.url);
        self.records.retain(|r| r.path != record.path);
        self.records.insert(0, record);
        if self.records.len() > MAX_DOWNLOAD_HISTORY {
            self.records.truncate(MAX_DOWNLOAD_HISTORY);
        }
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.records.len();
        self.records.retain(|r| r.id != id);
        self.records.len() != before
    }

    pub fn clear(&mut self) {
        self.records.clear();
    }
}

/// Extensiones compuestas habituales para conservar ambos sufijos al renombrar (`archivo (1).tar.gz`).
const COMPOUND_EXTENSIONS: [&str; 10] = [
    ".tar.gz",
    ".tar.bz2",
    ".tar.xz",
    ".tar.zst",
    ".tar.lz",
    ".tar.lzma",
    ".tar.br",
    ".user.js",
    ".appimage.zsync",
    ".svg.gz",
];

/// Sanea un nombre de archivo sugerido por el servidor eliminando separadores de ruta,
/// bytes NUL y nombres especiales (`.` / `..`).
pub fn sanitize_download_filename(raw: &str) -> String {
    let candidate = raw
        .trim()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .replace('\0', "");
    let cleaned: String = candidate
        .chars()
        .map(|c| if c.is_control() { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_start_matches('.');
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        "descarga".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Divide un nombre de archivo en `(base, ext_con_punto)` respetando extensiones compuestas
/// (`.tar.gz`) y archivos sin extensión (`Makefile`).
fn split_stem_and_ext(filename: &str) -> (&str, &str) {
    let lower = filename.to_ascii_lowercase();
    for compound in COMPOUND_EXTENSIONS {
        if lower.ends_with(compound) && filename.len() > compound.len() {
            let cut = filename.len() - compound.len();
            return (&filename[..cut], &filename[cut..]);
        }
    }
    if let Some(dot) = filename.rfind('.') {
        if dot > 0 && dot < filename.len() - 1 {
            return (&filename[..dot], &filename[dot..]);
        }
    }
    (filename, "")
}

/// Calcula una ruta destino única dentro de `dir` sin sobrescribir jamás archivos existentes:
/// `archivo.pdf` → `archivo (1).pdf` → `archivo (2).pdf` (preservando `.tar.gz` y archivos sin extensión).
pub fn unique_download_path(dir: &Path, suggested_filename: &str) -> PathBuf {
    let clean = sanitize_download_filename(suggested_filename);
    let direct = dir.join(&clean);
    if !path_occupied(&direct) {
        return direct;
    }

    let (stem, ext) = split_stem_and_ext(&clean);
    for counter in 1u32..=100_000 {
        let candidate_name = format!("{stem} ({counter}){ext}");
        let candidate = dir.join(&candidate_name);
        if !path_occupied(&candidate) {
            return candidate;
        }
    }

    dir.join(format!("{stem} (nuevo){ext}"))
}

fn path_occupied(path: &Path) -> bool {
    path.exists() || path.symlink_metadata().is_ok()
}

/// Formatea un tamaño en bytes a una cadena legible (`512 B`, `14.2 KB`, `2.4 MB`, `1.15 GB`).
pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;

    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

/// Formatea velocidad de transferencia en bytes/segundo (`420.0 KB/s`, `12.4 MB/s`).
pub fn format_speed(bytes_per_sec: u64) -> String {
    format!("{}/s", format_bytes(bytes_per_sec))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::DotPaths;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(1);

    fn temp_dir(tag: &str) -> PathBuf {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "search-downloads-test-{tag}-{}-{n}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn unique_download_path_never_overwrites_and_handles_extensions() {
        let dir = temp_dir("unique");

        let p0 = unique_download_path(&dir, "archivo.pdf");
        assert_eq!(p0, dir.join("archivo.pdf"));
        fs::write(&p0, b"1").expect("write p0");

        let p1 = unique_download_path(&dir, "archivo.pdf");
        assert_eq!(p1, dir.join("archivo (1).pdf"));
        fs::write(&p1, b"2").expect("write p1");

        let p2 = unique_download_path(&dir, "archivo.pdf");
        assert_eq!(p2, dir.join("archivo (2).pdf"));

        // Doble extensión (.tar.gz).
        let t0 = unique_download_path(&dir, "paquete.tar.gz");
        assert_eq!(t0, dir.join("paquete.tar.gz"));
        fs::write(&t0, b"tar").expect("write t0");
        let t1 = unique_download_path(&dir, "paquete.tar.gz");
        assert_eq!(t1, dir.join("paquete (1).tar.gz"));

        // Sin extensión.
        let n0 = unique_download_path(&dir, "README");
        assert_eq!(n0, dir.join("README"));
        fs::write(&n0, b"readme").expect("write n0");
        let n1 = unique_download_path(&dir, "README");
        assert_eq!(n1, dir.join("README (1)"));

        // Saneamiento de path traversal.
        let evil = unique_download_path(&dir, "../../etc/passwd");
        assert_eq!(evil, dir.join("passwd"));

        let empty = unique_download_path(&dir, "   ...  ");
        assert_eq!(empty, dir.join("descarga"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn format_bytes_and_speed_human_readable() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(2 * 1024 * 1024), "2.0 MB");
        assert_eq!(format_speed(1024 * 1024), "1.0 MB/s");
    }

    #[test]
    fn download_history_roundtrip_and_corrupt_bak_recovery() {
        let root = temp_dir("store");
        let sp = DotPaths::with_base(&root);
        sp.ensure_dirs().expect("ensure_dirs");
        let file = sp.downloads_file();

        let mut store = DownloadHistoryStore::new();
        store.add(DownloadRecord {
            id: "DL-1".to_string(),
            name: "manual (1).pdf".to_string(),
            path: "/tmp/manual (1).pdf".to_string(),
            url: "https://user:secret@example.com/manual.pdf".to_string(),
            bytes: 1_048_576,
            finished: 1_795_032_500.0,
        });
        store.save(&file).expect("save downloads");

        let loaded = DownloadHistoryStore::load(&file);
        assert_eq!(loaded.records().len(), 1);
        assert_eq!(loaded.records()[0].url, "https://example.com/manual.pdf");

        paths::write_atomic(&file, b"{corrupt").expect("write corrupt");
        let recovered = DownloadHistoryStore::load(&file);
        assert!(recovered.is_empty());
        assert!(file.with_extension("json.bak").exists());

        let _ = fs::remove_dir_all(&root);
    }
}
