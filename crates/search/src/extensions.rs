//! Gestor de almacenamiento CRX3 y estado de compatibilidad de extensiones en CEF 156 (Fase 9 — Paso 2B).
//!
//! El renderizado windowless OSR de CEF 156 (`CEF_RUNTIME_STYLE_ALLOY`) no expone
//! la API `CefExtension` (eliminada en CEF 128+) ni el subsistema `//chrome` para
//! ejecutar service workers MV3 / `chrome.runtime` sin mostrar la UI propia de Chrome.
//! Este módulo conecta el almacén verificado `search_core::extensions::ExtensionStore`
//! (`$XDG_DATA_HOME/search/extensions`) detrás del ajuste `extensions_enabled`
//! (desactivado por defecto) para preparar e inspeccionar paquetes CRX3 firmados.

use anyhow::Result;
use search_core::crx;
use search_core::extensions::{ExtensionStore, InstalledExtension};
use search_core::paths::SearchPaths;
use std::fs;
use std::path::{Path, PathBuf};

/// Indica si el runtime actual de CEF Alloy/OSR puede ejecutar extensiones MV3.
#[allow(dead_code)]
pub const fn is_mv3_runtime_supported() -> bool {
    false
}

/// Explicación técnica transparente para la interfaz y los logs.
pub const fn runtime_limitation_summary() -> &'static str {
    "CEF 156 en modo Alloy/OSR (windowless) no incluye la API CefExtension (eliminada en CEF 128+) ni ejecuta extensiones Chrome MV3 sin activar la interfaz de Chrome."
}

/// Carga el almacén de extensiones desde `$XDG_DATA_HOME/search/extensions`.
pub fn load_store(paths: &SearchPaths) -> Result<ExtensionStore> {
    ExtensionStore::load(paths.extensions_dir())
}

/// Instala y verifica criptográficamente un archivo `.crx` local en `$XDG_DATA_HOME/search/extensions`.
pub fn install_crx_file(
    paths: &SearchPaths,
    crx_path: &Path,
    expected_id: Option<&str>,
) -> Result<InstalledExtension> {
    let bytes = fs::read(crx_path)?;
    let mut store = load_store(paths)?;
    store.install_from_crx(expected_id, &bytes)
}

/// Activa o desactiva una extensión registrada en `installed.json`.
#[allow(dead_code)]
pub fn set_extension_enabled(paths: &SearchPaths, id: &str, enabled: bool) -> Result<bool> {
    let mut store = load_store(paths)?;
    store.set_enabled(id, enabled)
}

/// Elimina una extensión instalada de `installed.json` y de disco.
#[allow(dead_code)]
pub fn remove_extension(paths: &SearchPaths, id: &str) -> Result<bool> {
    let mut store = load_store(paths)?;
    store.remove(id)
}

/// Resuelve un ID o URL de Chrome Web Store y devuelve la URL oficial de descarga CRX3.
#[allow(dead_code)]
pub fn resolve_cws_download_url(input: &str) -> Option<(String, String)> {
    let id = crx::extract_extension_id(input)?;
    let url = crx::cws_download_url(&id).ok()?;
    Some((id, url.to_string()))
}

/// Ruta de almacenamiento de extensiones para mostrar en la interfaz.
pub fn storage_dir(paths: &SearchPaths) -> PathBuf {
    paths.extensions_dir()
}
