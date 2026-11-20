//! Historial de navegación (`$XDG_DATA_HOME/search/history.json`), puntuación por frecencia
//! (frecuencia × decaimiento exponencial de 30 días), índice en RAM para autocompletado
//! `< 16 ms` con 50.000 entradas, sugerencias unificadas del campo de dirección (historial,
//! pestañas abiertas y marcadores) y completado inline de dominios (`History.swift`).

use crate::address::{display_at_rest, sanitize_url_for_ui};
use crate::bookmarks::{normalize_bookmark_url, BookmarkStore};
use crate::paths;
use anyhow::Result;
use memchr::memmem::Finder;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use url::Url;

/// Constante de decaimiento exponencial de frecencia: 30 días en segundos (`History.swift`).
pub const FRECENCY_DECAY_SECS: f64 = 30.0 * 86_400.0;
const MAX_VISIT_COUNT: u64 = 1_000_000_000;

/// Entrada persistente de historial (`PORTING.md` §4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub url: String,
    pub key: String,
    #[serde(default)]
    pub title: String,
    pub count: u64,
    pub last: f64,
}

#[derive(Debug, Clone)]
struct IndexedVisit {
    entry: HistoryEntry,
    /// Dirección en minúsculas sin `https://`, `http://` ni `www.`.
    searchable: String,
    /// Host en minúsculas sin `www.`.
    host: String,
    /// Posición en bytes del final del host dentro de `searchable`.
    host_end: usize,
    /// Título en minúsculas precalculado.
    title_lower: String,
    /// Dirección compacta mostrable/editable sin esquema `https://` redundante.
    display_addr: String,
    /// `true` si la ruta es vacía o `"/"`.
    is_homepage: bool,
    /// Peso precalculado respecto a `HistoryStore::epoch`:
    /// `count * exp((last - epoch) / FRECENCY_DECAY_SECS)`.
    cached_weight: f64,
}

/// Información resumida de una pestaña abierta para el autocompletado del campo de dirección.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenTabInfo {
    pub window_id: u64,
    pub tab_id: u64,
    pub title: String,
    pub url: String,
    pub is_private: bool,
}

/// Tipo de sugerencia en el desplegable del omnibox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OmniboxSuggestionKind {
    OpenTab { window_id: u64, tab_id: u64 },
    Bookmark { id: String },
    History { key: String },
}

/// Sugerencia unificada del campo de dirección (historial, pestaña abierta o marcador).
#[derive(Debug, Clone, PartialEq)]
pub struct OmniboxSuggestion {
    pub kind: OmniboxSuggestionKind,
    /// Clave de dirección editable para completado inline (p. ej. `github.com/driceroland/Search`).
    pub completion_key: String,
    pub title: String,
    pub url: String,
    pub display_url: String,
    pub score: f64,
}

/// Grupo de entradas del historial por día de calendario (`Hoy`, `Ayer` o fecha).
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryDayGroup {
    pub label: String,
    pub entries: Vec<HistoryEntry>,
}

/// Almacén de historial indexado en memoria RAM para consultas de autocompletado en `< 16 ms`
/// incluso con 50.000+ entradas.
#[derive(Debug, Clone)]
pub struct HistoryStore {
    epoch: f64,
    entries: Vec<IndexedVisit>,
    /// Máscara compacta de caracteres por entrada (8 bytes/entrada, cabe entera en caché L2)
    /// para descartar en 1 operación bit a bit entradas sin los caracteres buscados.
    char_masks: Vec<u64>,
    by_key: HashMap<String, usize>,
}

impl Default for HistoryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl HistoryStore {
    pub fn new() -> Self {
        Self::with_epoch(crate::session::now_unix_secs())
    }

    pub fn with_epoch(epoch: f64) -> Self {
        Self {
            epoch,
            entries: Vec::new(),
            char_masks: Vec::new(),
            by_key: HashMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Carga `history.json` desde disco. Ante JSON corrupto guarda `<ruta>.bak`, emite `warn!`
    /// y arranca limpio.
    pub fn load(path: &Path) -> Self {
        let epoch = crate::session::now_unix_secs();
        if !path.exists() {
            return Self::with_epoch(epoch);
        }

        let raw = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(err) => {
                crate::warn!(
                    "No se pudo leer '{}': {err}. Arrancando historial vacío.",
                    path.display()
                );
                return Self::with_epoch(epoch);
            }
        };

        let list = match serde_json::from_str::<Vec<HistoryEntry>>(&raw) {
            Ok(v) => v,
            Err(err) => {
                crate::warn!(
                    "JSON corrupto en historial '{}': {err}. Guardando respaldo .bak.",
                    path.display()
                );
                if let Err(bak_err) = paths::backup_corrupt_file(path) {
                    crate::warn!("No se pudo respaldar '{}': {bak_err}", path.display());
                }
                return Self::with_epoch(epoch);
            }
        };

        let mut store = Self::with_epoch(epoch);
        store.entries.reserve(list.len());
        store.char_masks.reserve(list.len());
        store.by_key.reserve(list.len());
        for saved in list {
            store.insert_loaded_entry(saved);
        }
        store
    }

    /// Guarda el historial de forma atómica en `path`.
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut raw_entries: Vec<&HistoryEntry> = self.entries.iter().map(|i| &i.entry).collect();
        raw_entries.sort_by(|a, b| {
            b.last
                .partial_cmp(&a.last)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut json = serde_json::to_vec_pretty(&raw_entries)?;
        json.push(b'\n');
        paths::write_atomic(path, &json)
    }

    /// Registra una visita confirmada `http://` / `https://`.
    /// Ignora esquemas internos (`about:`, `data:`, `chrome-error:`, etc.).
    /// También acredita la página raíz del dominio para que el autocompletado ofrezca el host.
    pub fn record(&mut self, raw_url: &str, title: &str, now_unix_secs: f64) -> bool {
        let Some(prep) = prepare_record(raw_url) else {
            return false;
        };

        // Acreditar el dominio raíz al visitar páginas profundas (como `History.swift`).
        if !prep.is_homepage {
            if let Some((home_url, home_key)) = prep.home_pair {
                self.upsert_visit(&home_url, &home_key, "", 1, now_unix_secs, true);
            }
        }

        self.upsert_visit(
            &prep.clean_url,
            &prep.key,
            title.trim(),
            1,
            now_unix_secs,
            false,
        );
        true
    }

    /// Inserta directamente una entrada sintética o importada (útil para migraciones y benchmarks).
    pub fn insert_entry(&mut self, entry: HistoryEntry) {
        self.insert_loaded_entry(entry);
    }

    /// Actualiza el título de una visita existente cuando `on_title_change` llega tras la carga.
    pub fn retitle(&mut self, raw_url: &str, title: &str) -> bool {
        let clean_title = title.trim();
        if clean_title.is_empty() {
            return false;
        }
        let Some(prep) = prepare_record(raw_url) else {
            return false;
        };
        let Some(&idx) = self.by_key.get(&prep.key) else {
            return false;
        };
        let item = &mut self.entries[idx];
        if item.entry.title == clean_title {
            return false;
        }
        item.entry.title = clean_title.to_string();
        item.title_lower = clean_title.to_lowercase();
        self.char_masks[idx] = compute_char_mask(&item.searchable, &item.title_lower);
        true
    }

    /// Borra una entrada individual por su clave normalizada `key`.
    pub fn remove_by_key(&mut self, key: &str) -> bool {
        let Some(idx) = self.by_key.remove(key) else {
            return false;
        };
        self.entries.swap_remove(idx);
        self.char_masks.swap_remove(idx);
        if idx < self.entries.len() {
            let moved_key = self.entries[idx].entry.key.clone();
            self.by_key.insert(moved_key, idx);
        }
        true
    }

    /// Borra todas las entradas pertenecientes a `host` (incluyendo `www.<host>` y subdominios).
    pub fn remove_host(&mut self, host: &str) -> usize {
        let target = bare_host(host);
        if target.is_empty() {
            return 0;
        }
        let suffix = format!(".{target}");
        let before = self.entries.len();
        self.entries
            .retain(|v| v.host != target && !v.host.ends_with(&suffix));
        let removed = before - self.entries.len();
        if removed > 0 {
            self.rebuild_indices();
        }
        removed
    }

    /// Borra todas las entradas con `last >= cutoff_unix_secs` (p. ej. última hora o último día).
    pub fn clear_since(&mut self, cutoff_unix_secs: f64) -> usize {
        let before = self.entries.len();
        self.entries.retain(|v| v.entry.last < cutoff_unix_secs);
        let removed = before - self.entries.len();
        if removed > 0 {
            self.rebuild_indices();
        }
        removed
    }

    /// Vacía completamente el historial.
    pub fn clear_all(&mut self) {
        self.entries.clear();
        self.char_masks.clear();
        self.by_key.clear();
    }

    /// Devuelve entradas ordenadas de la más reciente a la más antigua filtradas por `query`,
    /// omitiendo créditos automáticos sin título de portada para no duplicar líneas.
    pub fn filter_chronological(&self, query: &str, limit: usize) -> Vec<HistoryEntry> {
        let needle = query.trim().to_lowercase();
        let finder = (!needle.is_empty()).then(|| Finder::new(needle.as_bytes()));

        let mut matched: Vec<&HistoryEntry> = self
            .entries
            .iter()
            .filter(|v| !(v.is_homepage && v.entry.title.is_empty()))
            .filter(|v| match finder {
                None => true,
                Some(ref f) => {
                    f.find(v.searchable.as_bytes()).is_some()
                        || f.find(v.title_lower.as_bytes()).is_some()
                }
            })
            .map(|v| &v.entry)
            .collect();

        matched.sort_by(|a, b| {
            b.last
                .partial_cmp(&a.last)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        if limit > 0 && matched.len() > limit {
            matched.truncate(limit);
        }
        matched.into_iter().cloned().collect()
    }

    /// Busca las mejores `limit` coincidencias de historial por frecencia + calidad de coincidencia.
    /// Diseñado para escanear 50.000 entradas con cero asignaciones en el bucle interno (`< 16 ms`).
    pub fn suggest(&self, typed: &str, limit: usize, now_unix_secs: f64) -> Vec<OmniboxSuggestion> {
        if limit == 0 || self.entries.is_empty() {
            return Vec::new();
        }
        let needle = stripped_address(typed);
        if needle.is_empty() {
            return Vec::new();
        }

        let needle_bytes = needle.as_bytes();
        let words: Vec<&[u8]> = needle.split_whitespace().map(str::as_bytes).collect();
        if words.is_empty() {
            return Vec::new();
        }
        let finders: Vec<Finder<'_>> = words.iter().map(Finder::new).collect();
        let query_mask = compute_query_mask(&words);
        let fade = (-(now_unix_secs - self.epoch) / FRECENCY_DECAY_SECS).exp();

        let mut top: Vec<(usize, f64, usize)> = Vec::with_capacity(limit + 1);

        for (idx, &mask) in self.char_masks.iter().enumerate() {
            if (mask & query_mask) != query_mask {
                continue;
            }
            let item = &self.entries[idx];
            let s_bytes = item.searchable.as_bytes();
            let t_bytes = item.title_lower.as_bytes();

            let fit = if finders.len() == 1 {
                compute_fit_single(needle_bytes, &finders[0], s_bytes, item.host_end, t_bytes)
            } else {
                compute_fit_multi(&words, &finders, s_bytes, t_bytes)
            };

            let Some(fit) = fit else {
                continue;
            };

            let frecency = (item.cached_weight * fade).max(0.0);
            let home_bonus = if item.is_homepage && fit >= 3.0 {
                1.08
            } else {
                1.0
            };
            let score = fit * home_bonus * (2.0 + frecency).ln();
            let addr_len = item.display_addr.len();

            if top.len() == limit {
                if let Some(&(_, min_score, min_len)) = top.last() {
                    if score < min_score || (score == min_score && addr_len >= min_len) {
                        continue;
                    }
                }
            }

            let pos = top
                .iter()
                .position(|&(_, s, l)| score > s || (score == s && addr_len < l))
                .unwrap_or(top.len());
            top.insert(pos, (idx, score, addr_len));
            if top.len() > limit {
                top.pop();
            }
        }

        top.into_iter()
            .map(|(idx, score, _)| {
                let item = &self.entries[idx];
                OmniboxSuggestion {
                    kind: OmniboxSuggestionKind::History {
                        key: item.entry.key.clone(),
                    },
                    completion_key: item.display_addr.clone(),
                    title: if item.entry.title.is_empty() {
                        item.host.clone()
                    } else {
                        item.entry.title.clone()
                    },
                    url: item.entry.url.clone(),
                    display_url: item.display_addr.clone(),
                    score,
                }
            })
            .collect()
    }

    fn insert_loaded_entry(&mut self, saved: HistoryEntry) {
        let Some(prep) = prepare_record(&saved.url) else {
            return;
        };
        let count = saved.count.min(MAX_VISIT_COUNT);
        self.upsert_visit(
            &prep.clean_url,
            &prep.key,
            saved.title.trim(),
            count,
            saved.last,
            true,
        );
    }

    fn upsert_visit(
        &mut self,
        clean_url: &str,
        key: &str,
        title: &str,
        delta_count: u64,
        last: f64,
        merge_max_last: bool,
    ) {
        if let Some(&idx) = self.by_key.get(key) {
            let item = &mut self.entries[idx];
            item.entry.count = item
                .entry
                .count
                .saturating_add(delta_count)
                .min(MAX_VISIT_COUNT);
            item.entry.last = if merge_max_last {
                item.entry.last.max(last)
            } else {
                last
            };
            item.entry.url = clean_url.to_string();
            if !title.is_empty() {
                item.entry.title = title.to_string();
                item.title_lower = title.to_lowercase();
                self.char_masks[idx] = compute_char_mask(&item.searchable, &item.title_lower);
            }
            item.cached_weight = frecency_weight(item.entry.count, item.entry.last, self.epoch);
            return;
        }

        let indexed = build_indexed_visit(
            HistoryEntry {
                url: clean_url.to_string(),
                key: key.to_string(),
                title: title.to_string(),
                count: delta_count.min(MAX_VISIT_COUNT),
                last,
            },
            self.epoch,
        );
        let mask = compute_char_mask(&indexed.searchable, &indexed.title_lower);
        let idx = self.entries.len();
        self.entries.push(indexed);
        self.char_masks.push(mask);
        self.by_key.insert(key.to_string(), idx);
    }

    fn rebuild_indices(&mut self) {
        self.by_key.clear();
        self.by_key.reserve(self.entries.len());
        self.char_masks.clear();
        self.char_masks.reserve(self.entries.len());
        for (idx, item) in self.entries.iter().enumerate() {
            self.by_key.insert(item.entry.key.clone(), idx);
            self.char_masks
                .push(compute_char_mask(&item.searchable, &item.title_lower));
        }
    }
}

/// Devuelve `true` si la URL debe registrarse en el historial (solo `http://` y `https://`,
/// excluyendo páginas internas, errores, `about:`, `data:`, `chrome-error:`, etc.).
pub fn is_recordable_url(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("about:")
        || trimmed.starts_with("data:")
        || trimmed.starts_with("chrome-error:")
        || trimmed.starts_with("chrome:")
        || trimmed.starts_with("javascript:")
        || trimmed.starts_with("blob:")
    {
        return false;
    }
    let Ok(parsed) = Url::parse(trimmed) else {
        return false;
    };
    matches!(parsed.scheme(), "http" | "https") && parsed.host_str().is_some()
}

/// Quita el esquema `http(s)://` y `www.` y pasa a minúsculas para comparaciones de autocompletado.
pub fn stripped_address(raw: &str) -> String {
    let trimmed = raw.trim().to_lowercase();
    let no_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(&trimmed);
    no_scheme
        .strip_prefix("www.")
        .unwrap_or(no_scheme)
        .to_string()
}

/// Calcula el peso de frecencia exacto `count * exp(-days / 30)` para una marca temporal `now`.
pub fn frecency_score(count: u64, last_unix_secs: f64, now_unix_secs: f64) -> f64 {
    let age_secs = (now_unix_secs - last_unix_secs).max(0.0);
    (count as f64) * (-age_secs / FRECENCY_DECAY_SECS).exp()
}

fn frecency_weight(count: u64, last: f64, epoch: f64) -> f64 {
    (count as f64) * ((last - epoch) / FRECENCY_DECAY_SECS).exp()
}

fn bare_host(host: &str) -> String {
    let lower = host.trim().to_ascii_lowercase();
    lower.strip_prefix("www.").unwrap_or(&lower).to_string()
}

struct PreparedVisit {
    clean_url: String,
    key: String,
    is_homepage: bool,
    home_pair: Option<(String, String)>,
}

fn prepare_record(raw_url: &str) -> Option<PreparedVisit> {
    if !is_recordable_url(raw_url) {
        return None;
    }
    let mut parsed = Url::parse(raw_url.trim()).ok()?;
    let _ = parsed.set_username("");
    let _ = parsed.set_password(None);
    let clean_url = parsed.to_string();

    let mut key_url = parsed.clone();
    key_url.set_fragment(None);
    if key_url.path().is_empty() {
        key_url.set_path("/");
    }
    let key = key_url.to_string();
    let is_homepage =
        (key_url.path().is_empty() || key_url.path() == "/") && key_url.query().is_none();

    let home_pair = if !is_homepage {
        let mut home = key_url.clone();
        home.set_path("/");
        home.set_query(None);
        home.set_fragment(None);
        let h_str = home.to_string();
        Some((h_str.clone(), h_str))
    } else {
        None
    };

    Some(PreparedVisit {
        clean_url,
        key,
        is_homepage,
        home_pair,
    })
}

fn build_indexed_visit(entry: HistoryEntry, epoch: f64) -> IndexedVisit {
    let display_addr = editable_address(&entry.url);
    let searchable = stripped_address(&display_addr);
    let host_end = searchable
        .bytes()
        .position(|b| b == b'/' || b == b'?' || b == b'#')
        .unwrap_or(searchable.len());
    let host = searchable[..host_end]
        .split(':')
        .next()
        .unwrap_or(&searchable[..host_end])
        .to_string();
    let is_homepage = Url::parse(&entry.url)
        .map(|u| (u.path().is_empty() || u.path() == "/") && u.query().is_none())
        .unwrap_or(false);
    let title_lower = entry.title.to_lowercase();
    let cached_weight = frecency_weight(entry.count, entry.last, epoch);

    IndexedVisit {
        entry,
        searchable,
        host,
        host_end,
        title_lower,
        display_addr,
        is_homepage,
        cached_weight,
    }
}

/// Dirección legible/editable para sugerencias (quita `https://` y `/` final en portadas).
pub fn editable_address(raw_url: &str) -> String {
    let clean = sanitize_url_for_ui(raw_url);
    if let Some(rest) = clean.strip_prefix("https://") {
        let without_www = rest.strip_prefix("www.").unwrap_or(rest);
        if let Some(stripped_slash) = without_www.strip_suffix('/') {
            if !stripped_slash.contains('/') && !stripped_slash.contains('?') {
                return stripped_slash.to_string();
            }
        }
        return without_www.to_string();
    }
    if let Some(rest) = clean.strip_prefix("http://") {
        if let Some(stripped_slash) = rest.strip_suffix('/') {
            if !stripped_slash.contains('/') && !stripped_slash.contains('?') {
                return format!("http://{stripped_slash}");
            }
        }
    }
    clean
}

#[inline]
fn char_bit(b: u8) -> u64 {
    match b {
        b'a'..=b'z' => 1u64 << (b - b'a'),
        b'0'..=b'9' => 1u64 << (26 + (b - b'0')),
        b'.' => 1u64 << 36,
        b'/' => 1u64 << 37,
        b'-' => 1u64 << 38,
        b'_' => 1u64 << 39,
        b'?' => 1u64 << 40,
        b'=' => 1u64 << 41,
        b':' => 1u64 << 42,
        _ if b >= 0x80 => 1u64 << 43,
        _ => 0,
    }
}

fn compute_char_mask(searchable: &str, title_lower: &str) -> u64 {
    let mut mask = 0u64;
    for &b in searchable.as_bytes() {
        mask |= char_bit(b);
    }
    for &b in title_lower.as_bytes() {
        mask |= char_bit(b);
    }
    mask
}

fn compute_query_mask(words: &[&[u8]]) -> u64 {
    let mut mask = 0u64;
    for w in words {
        for &b in *w {
            mask |= char_bit(b);
        }
    }
    mask
}

#[inline]
fn is_word_sep(b: u8) -> bool {
    !b.is_ascii_alphanumeric()
}

#[inline]
fn starts_word_in(finder: &Finder<'_>, haystack: &[u8]) -> bool {
    let mut offset = 0;
    while let Some(rel) = finder.find(&haystack[offset..]) {
        let pos = offset + rel;
        if pos == 0 || is_word_sep(haystack[pos - 1]) {
            return true;
        }
        offset = pos + 1;
        if offset >= haystack.len() {
            break;
        }
    }
    false
}

#[inline]
fn compute_fit_single(
    needle: &[u8],
    finder: &Finder<'_>,
    searchable: &[u8],
    host_end: usize,
    title: &[u8],
) -> Option<f64> {
    // 1. Prefijo exacto de la dirección/host (tier 4).
    if searchable.starts_with(needle) {
        return Some(4.0);
    }

    // 2. Búsqueda única en `searchable` (sin re-escanear tres veces el mismo buffer).
    let mut best_url_fit: Option<f64> = None;
    if let Some(pos) = finder.find(searchable) {
        let is_boundary = pos == 0 || is_word_sep(searchable[pos - 1]);
        if pos < host_end && is_boundary {
            return Some(3.0);
        }
        if needle.len() >= 2 {
            if is_boundary {
                return Some(2.0);
            }
            if needle.len() >= 3 {
                best_url_fit = Some(1.0);
            }
        }
    }

    // Con menos de 2 bytes solo ofrecemos coincidencias por prefijo/host para evitar ruido.
    if needle.len() < 2 {
        return None;
    }

    // 3. Búsqueda única en `title` (solo si no hubo ya coincidencia de tier >= 2 en URL).
    if let Some(pos) = finder.find(title) {
        if pos == 0 || is_word_sep(title[pos - 1]) {
            return Some(2.0);
        }
        if needle.len() >= 3 {
            return Some(1.0);
        }
    }

    best_url_fit
}

#[inline]
fn compute_fit_multi(
    words: &[&[u8]],
    finders: &[Finder<'_>],
    searchable: &[u8],
    title: &[u8],
) -> Option<f64> {
    let mut all_word_start = true;
    for (word, finder) in words.iter().zip(finders.iter()) {
        let in_search = finder.find(searchable).is_some();
        let in_title = !in_search && finder.find(title).is_some();
        if !in_search && !in_title {
            return None;
        }
        if all_word_start
            && !starts_word_in(finder, searchable)
            && !starts_word_in(finder, title)
            && !searchable.starts_with(word)
        {
            all_word_start = false;
        }
    }
    if all_word_start {
        Some(2.2)
    } else {
        Some(1.0)
    }
}

/// Combina sugerencias del historial (frecencia), pestañas abiertas ("Cambiar a pestaña")
/// y marcadores para el desplegable del campo de dirección.
pub fn suggest_omnibox(
    typed: &str,
    history: &HistoryStore,
    bookmarks: &BookmarkStore,
    open_tabs: &[OpenTabInfo],
    limit: usize,
    now_unix_secs: f64,
) -> Vec<OmniboxSuggestion> {
    let needle_raw = typed.trim();
    if needle_raw.is_empty() || limit == 0 {
        return Vec::new();
    }
    let needle_lower = needle_raw.to_lowercase();
    let needle_bare = stripped_address(needle_raw);
    let words: Vec<&str> = needle_lower.split_whitespace().collect();

    let mut out: Vec<OmniboxSuggestion> = Vec::with_capacity(limit * 2);
    let mut seen_urls: HashSet<String> = HashSet::new();

    // 1. Pestañas abiertas (solo desde 2 caracteres o prefijo de host).
    for tab in open_tabs {
        if !is_recordable_url(&tab.url) {
            continue;
        }
        let disp = editable_address(&tab.url);
        let bare = stripped_address(&disp);
        let title_low = tab.title.to_lowercase();
        let matches = if needle_bare.len() == 1 {
            bare.starts_with(&needle_bare)
        } else {
            words
                .iter()
                .all(|w| bare.contains(w) || title_low.contains(w))
        };
        if !matches {
            continue;
        }
        let norm = normalize_bookmark_url(&tab.url);
        if !seen_urls.insert(norm) {
            continue;
        }
        let fit = if bare.starts_with(&needle_bare) {
            16.0
        } else if title_low.starts_with(&needle_lower) {
            12.0
        } else {
            9.0
        };
        out.push(OmniboxSuggestion {
            kind: OmniboxSuggestionKind::OpenTab {
                window_id: tab.window_id,
                tab_id: tab.tab_id,
            },
            completion_key: disp.clone(),
            title: if tab.title.trim().is_empty() {
                display_at_rest(&tab.url)
            } else {
                tab.title.clone()
            },
            url: tab.url.clone(),
            display_url: disp,
            score: fit,
        });
    }

    // 2. Marcadores (desde 2 caracteres, como `Bookmarks.swift`).
    if needle_raw.chars().count() >= 2 {
        for bm in bookmarks.search(needle_raw, false, limit) {
            let Some(ref url) = bm.node.url else {
                continue;
            };
            let norm = normalize_bookmark_url(url);
            if !seen_urls.insert(norm) {
                continue;
            }
            let disp = editable_address(url);
            let score = match bm.score {
                3 => 14.0,
                2 => 10.0,
                _ => 7.0,
            };
            let label = bm.node.display_label();
            out.push(OmniboxSuggestion {
                kind: OmniboxSuggestionKind::Bookmark { id: bm.node.id },
                completion_key: disp.clone(),
                title: label,
                url: url.clone(),
                display_url: disp,
                score,
            });
        }
    }

    // 3. Historial puntuado por frecencia.
    for h in history.suggest(typed, limit + 4, now_unix_secs) {
        let norm = normalize_bookmark_url(&h.url);
        if seen_urls.insert(norm) {
            out.push(h);
        }
    }

    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.completion_key.len().cmp(&b.completion_key.len()))
    });
    if out.len() > limit {
        out.truncate(limit);
    }
    out
}

/// Calcula el sufijo de autocompletado inline que se añade y selecciona tras el cursor
/// mientras el usuario teclea hacia delante (`History.swift::completion`).
pub fn inline_completion(typed: &str, suggestions: &[OmniboxSuggestion]) -> Option<String> {
    if typed.is_empty() || typed.chars().any(char::is_whitespace) || suggestions.is_empty() {
        return None;
    }

    let typed_chars: Vec<char> = typed.chars().collect();
    let single_char = typed_chars.len() == 1;
    let candidates = if single_char {
        // Con una sola letra solo completamos la portada del primer resultado visitado.
        if !matches!(suggestions[0].kind, OmniboxSuggestionKind::History { .. }) {
            return None;
        }
        &suggestions[..1]
    } else {
        suggestions
    };

    for opt in candidates {
        let mut forms: Vec<String> = vec![opt.completion_key.clone()];
        if let Some(rest) = opt.url.strip_prefix("https://") {
            let trimmed = rest.strip_suffix('/').unwrap_or(rest);
            forms.push(format!("https://{trimmed}"));
            if trimmed.starts_with("www.") {
                forms.push(trimmed.to_string());
            }
        } else if let Some(rest) = opt.url.strip_prefix("http://") {
            let trimmed = rest.strip_suffix('/').unwrap_or(rest);
            forms.push(format!("http://{trimmed}"));
        }

        for form in forms {
            if single_char && (form.contains('/') || form.contains('?') || form.contains('#')) {
                continue;
            }
            let form_chars: Vec<char> = form.chars().collect();
            if form_chars.len() <= typed_chars.len() {
                continue;
            }
            let prefix_matches = typed_chars
                .iter()
                .zip(form_chars.iter())
                .all(|(a, b)| a.to_lowercase().eq(b.to_lowercase()));
            if prefix_matches {
                let suffix: String = form_chars[typed_chars.len()..].iter().collect();
                if !suffix.is_empty() {
                    return Some(suffix);
                }
            }
        }
    }

    None
}

/// Agrupa entradas cronológicas por día de calendario local: `"Hoy"`, `"Ayer"` o fecha legible.
pub fn group_by_day(entries: &[HistoryEntry], now_unix_secs: f64) -> Vec<HistoryDayGroup> {
    let today = local_ymd(now_unix_secs as i64);
    let yesterday = local_ymd((now_unix_secs as i64).saturating_sub(86_400));

    let mut groups: Vec<HistoryDayGroup> = Vec::new();
    let mut last_key: Option<(i32, u32, u32)> = None;

    for entry in entries {
        let ymd = local_ymd(entry.last as i64);
        if last_key != Some(ymd) {
            let label = if ymd == today {
                "Hoy".to_string()
            } else if ymd == yesterday {
                "Ayer".to_string()
            } else {
                format_spanish_date(ymd)
            };
            groups.push(HistoryDayGroup {
                label,
                entries: Vec::new(),
            });
            last_key = Some(ymd);
        }
        if let Some(last_group) = groups.last_mut() {
            last_group.entries.push(entry.clone());
        }
    }

    groups
}

/// Formatea una marca temporal Unix en hora local `HH:MM`.
pub fn format_time_hm(unix_secs: f64) -> String {
    let tm = local_tm(unix_secs as i64);
    format!(
        "{:02}:{:02}",
        tm.tm_hour.clamp(0, 23),
        tm.tm_min.clamp(0, 59)
    )
}

fn local_tm(secs: i64) -> libc::tm {
    let t = secs as libc::time_t;
    let mut out: libc::tm = unsafe { std::mem::zeroed() };
    unsafe {
        libc::localtime_r(&t, &mut out);
    }
    out
}

fn local_ymd(secs: i64) -> (i32, u32, u32) {
    let tm = local_tm(secs);
    (
        tm.tm_year + 1900,
        (tm.tm_mon + 1).max(1) as u32,
        tm.tm_mday.max(1) as u32,
    )
}

fn format_spanish_date((year, month, day): (i32, u32, u32)) -> String {
    const MONTHS: [&str; 12] = [
        "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic",
    ];
    let m = MONTHS
        .get(month.saturating_sub(1) as usize)
        .copied()
        .unwrap_or("?");
    format!("{day} {m} {year}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::DotPaths;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    static SEQ: AtomicU64 = AtomicU64::new(1);

    fn temp_history_path(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "search-history-test-{tag}-{}-{n}",
            std::process::id()
        ));
        let sp = DotPaths::with_base(&root);
        sp.ensure_dirs().expect("ensure_dirs");
        (root, sp.history_file())
    }

    #[test]
    fn filters_internal_urls_and_strips_credentials() {
        assert!(!is_recordable_url("about:blank"));
        assert!(!is_recordable_url("data:text/html,<h1>hola</h1>"));
        assert!(!is_recordable_url("chrome-error://chromewebdata/"));
        assert!(!is_recordable_url("javascript:void(0)"));
        assert!(is_recordable_url("https://example.com/path"));

        let now = 1_800_000_000.0;
        let mut store = HistoryStore::with_epoch(now);
        assert!(!store.record("about:blank", "Blank", now));
        assert!(store.record("https://user:pass@example.com/docs#sec", "Docs", now));
        let all = store.filter_chronological("", 10);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].url, "https://example.com/docs#sec");
        assert_eq!(all[0].key, "https://example.com/docs");
    }

    #[test]
    fn frecency_scoring_ranks_frequent_recent_and_prefix_first() {
        let now = 1_800_000_000.0;
        let mut store = HistoryStore::with_epoch(now);

        // Sitio visitado muchas veces recientemente.
        store.insert_entry(HistoryEntry {
            url: "https://github.com/".to_string(),
            key: "https://github.com/".to_string(),
            title: "GitHub".to_string(),
            count: 35,
            last: now - 600.0,
        });
        // Sitio reciente visitado pocas veces (prefijo exacto de host).
        store.insert_entry(HistoryEntry {
            url: "https://git-mirror.example/".to_string(),
            key: "https://git-mirror.example/".to_string(),
            title: "Git Mirror".to_string(),
            count: 3,
            last: now - 300.0,
        });
        // Página con misma frecuencia/recencia que solo menciona "git" en el título.
        store.insert_entry(HistoryEntry {
            url: "https://blog.example/tutorial".to_string(),
            key: "https://blog.example/tutorial".to_string(),
            title: "Git basics tutorial".to_string(),
            count: 3,
            last: now - 300.0,
        });
        // Sitio muy antiguo visitado una sola vez.
        store.insert_entry(HistoryEntry {
            url: "https://git-old.example/".to_string(),
            key: "https://git-old.example/".to_string(),
            title: "Old Git Mirror".to_string(),
            count: 1,
            last: now - 90.0 * 86_400.0,
        });

        let results = store.suggest("git", 5, now);
        assert_eq!(results.len(), 4);
        assert_eq!(results[0].url, "https://github.com/");
        assert_eq!(results[1].url, "https://git-mirror.example/");
        assert!(results[0].score > results[1].score);
        // Con misma frecuencia/recencia, el prefijo de host supera a coincidencia solo en título.
        let mirror_pos = results
            .iter()
            .position(|r| r.url == "https://git-mirror.example/")
            .expect("mirror");
        let blog_pos = results
            .iter()
            .position(|r| r.url == "https://blog.example/tutorial")
            .expect("blog");
        assert!(mirror_pos < blog_pos);

        // Completado inline para "git" → "hub.com".
        let comp = inline_completion("git", &results);
        assert_eq!(comp.as_deref(), Some("hub.com"));
        // Completado inline respetando prefijo https://
        let comp_https = inline_completion("https://git", &results);
        assert_eq!(comp_https.as_deref(), Some("hub.com"));
    }

    #[test]
    fn delete_entry_delete_host_and_clear_ranges() {
        let now = 1_800_000_000.0;
        let mut store = HistoryStore::with_epoch(now);
        store.record(
            "https://news.ycombinator.com/item?id=1",
            "HN 1",
            now - 300.0,
        );
        store.record(
            "https://www.ycombinator.com/companies",
            "YC",
            now - 10_000.0,
        );
        store.record("https://rust-lang.org/", "Rust", now - 200_000.0);

        // Borrar última hora solo borra la visita de hace 5 min (+ su crédito de portada).
        let cleared = store.clear_since(now - 3600.0);
        assert!(cleared >= 1);
        assert!(store.filter_chronological("item?id=1", 10).is_empty());

        // Borrar sitio entero.
        let removed_site = store.remove_host("ycombinator.com");
        assert!(removed_site >= 1);
        assert!(store.filter_chronological("ycombinator", 10).is_empty());

        // Rust de hace 2 días sigue intacto.
        assert_eq!(store.filter_chronological("rust", 10).len(), 1);
    }

    #[test]
    fn omnibox_combines_open_tabs_bookmarks_and_history() {
        let now = 1_800_000_000.0;
        let mut history = HistoryStore::with_epoch(now);
        history.record("https://docs.rs/cef/", "cef - Rust", now);

        let mut bookmarks = BookmarkStore::new();
        bookmarks.add_bookmark(
            "Rust Reference",
            "https://doc.rust-lang.org/reference/",
            None,
        );

        let open_tabs = vec![OpenTabInfo {
            window_id: 1,
            tab_id: 42,
            title: "Rust Playground".to_string(),
            url: "https://play.rust-lang.org/".to_string(),
            is_private: false,
        }];

        let items = suggest_omnibox("rust", &history, &bookmarks, &open_tabs, 6, now);
        assert_eq!(items.len(), 3);
        assert!(items.iter().any(|s| matches!(
            s.kind,
            OmniboxSuggestionKind::OpenTab {
                window_id: 1,
                tab_id: 42
            }
        )));
        assert!(items
            .iter()
            .any(|s| matches!(s.kind, OmniboxSuggestionKind::Bookmark { .. })));
        assert!(items
            .iter()
            .any(|s| matches!(s.kind, OmniboxSuggestionKind::History { .. })));
    }

    #[test]
    fn json_persistence_and_corrupt_bak_recovery() {
        let (root, file) = temp_history_path("persist");
        let now = 1_800_000_000.0;
        let mut store = HistoryStore::with_epoch(now);
        store.record("https://example.org/hello", "Hola Mundo", now);
        store.save(&file).expect("save history");

        let loaded = HistoryStore::load(&file);
        let items = loaded.filter_chronological("hola", 10);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].url, "https://example.org/hello");

        paths::write_atomic(&file, b"{not json").expect("corrupt");
        let recovered = HistoryStore::load(&file);
        assert!(recovered.is_empty());
        assert!(file.with_extension("json.bak").exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn autocomplete_50k_entries_completes_under_16ms() {
        let now = 1_800_000_000.0;
        let mut store = HistoryStore::with_epoch(now);
        store.entries.reserve(50_000);
        store.by_key.reserve(50_000);

        for i in 0..50_000u64 {
            let domain = match i % 25 {
                0 => "github.com",
                1 => "docs.rs",
                2 => "wikipedia.org",
                3 => "gnome.org",
                4 => "mozilla.org",
                _ => "site.example",
            };
            let url = format!("https://{domain}/article/{i}");
            let title = format!("Artículo técnico número {i} sobre {domain}");
            let entry = HistoryEntry {
                url: url.clone(),
                key: url,
                title,
                count: (i % 40) + 1,
                last: now - ((i % 90) as f64) * 3_600.0,
            };
            store.insert_entry(entry);
        }
        assert_eq!(store.len(), 50_000);

        // Calentamiento breve del caché L1/L2 y medición de consulta real.
        let _ = store.suggest("git", 8, now);
        let start = Instant::now();
        let results = store.suggest("github.com/article/499", 8, now);
        let elapsed_specific = start.elapsed();

        let start_broad = Instant::now();
        let broad_results = store.suggest("git", 8, now);
        let elapsed_broad = start_broad.elapsed();

        eprintln!(
            "[bench] autocomplete 50k entries: specific={:.3} ms ({} res), broad 'git'={:.3} ms ({} res)",
            elapsed_specific.as_secs_f64() * 1000.0,
            results.len(),
            elapsed_broad.as_secs_f64() * 1000.0,
            broad_results.len()
        );
        assert!(!results.is_empty());
        assert_eq!(broad_results.len(), 8);
        assert!(
            elapsed_specific < Duration::from_millis(16),
            "La consulta específica con 50.000 entradas tardó {:?} (límite < 16 ms)",
            elapsed_specific
        );
        assert!(
            elapsed_broad < Duration::from_millis(16),
            "La consulta amplia 'git' con 50.000 entradas tardó {:?} (límite < 16 ms)",
            elapsed_broad
        );
    }
}
