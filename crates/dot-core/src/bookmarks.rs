//! Árbol recursivo de marcadores (`$XDG_DATA_HOME/search/bookmarks.json`) e importación/exportación
//! en formato HTML de marcadores de Netscape compatible con Firefox y Chrome
//! (`Bookmarks.swift` + `BookmarksFile.swift`, esquema en `PORTING.md` §4).

use crate::address::{display_at_rest, sanitize_url_for_ui};
use crate::paths;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static ID_SEQ: AtomicU64 = AtomicU64::new(1);

/// Genera un UUID v4 en formato estándar (mayúsculas, compatible con `PORTING.md` §4).
pub fn generate_uuid() -> String {
    let mut bytes = [0u8; 16];
    let filled = fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .is_ok();
    if !filled {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let seq = u128::from(ID_SEQ.fetch_add(1, Ordering::Relaxed));
        let pid = u128::from(std::process::id());
        let mixed = nanos ^ (seq.wrapping_mul(0x9e37_79b9_7f4a_7c15)) ^ (pid << 64);
        bytes = mixed.to_be_bytes();
    }
    // Fijar bits de versión 4 y variante RFC 4122.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    format!(
        "{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15],
    )
}

/// Nodo de marcador o carpeta en `bookmarks.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookmarkNode {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<BookmarkNode>>,
}

impl BookmarkNode {
    pub fn site(title: impl Into<String>, url: impl Into<String>) -> Self {
        let url_str = sanitize_url_for_ui(url.into().trim());
        let raw_title = title.into();
        let clean_title = if raw_title.trim().is_empty() {
            let rest = display_at_rest(&url_str);
            if rest.is_empty() {
                url_str.clone()
            } else {
                rest
            }
        } else {
            raw_title.trim().to_string()
        };
        Self {
            id: generate_uuid(),
            title: clean_title,
            url: Some(url_str),
            children: None,
        }
    }

    pub fn folder(title: impl Into<String>, children: Vec<BookmarkNode>) -> Self {
        let t = title.into();
        let clean = t.trim();
        Self {
            id: generate_uuid(),
            title: if clean.is_empty() {
                "Carpeta".to_string()
            } else {
                clean.to_string()
            },
            url: None,
            children: Some(children),
        }
    }

    pub fn is_folder(&self) -> bool {
        self.url.is_none()
    }

    pub fn display_label(&self) -> String {
        let trimmed = self.title.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
        if let Some(ref u) = self.url {
            let rest = display_at_rest(u);
            if !rest.is_empty() {
                return rest;
            }
            return u.clone();
        }
        "Carpeta".to_string()
    }
}

/// Entrada plana de carpeta con profundidad para selectores de carpeta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderOption {
    pub id: String,
    pub title: String,
    pub depth: usize,
}

/// Resultado de búsqueda de un marcador junto con la ruta de carpetas padre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookmarkMatch {
    pub node: BookmarkNode,
    pub folder_path: Vec<String>,
    pub score: u32,
}

/// Resumen de importación HTML Netscape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BookmarkImportStats {
    pub added: usize,
    pub skipped: usize,
}

/// Almacén en memoria del árbol de marcadores con persistencia atómica y respaldo `.bak`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BookmarkStore {
    roots: Vec<BookmarkNode>,
}

impl BookmarkStore {
    pub fn new() -> Self {
        Self { roots: Vec::new() }
    }

    pub fn from_roots(roots: Vec<BookmarkNode>) -> Self {
        Self { roots }
    }

    pub fn roots(&self) -> &[BookmarkNode] {
        &self.roots
    }

    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    /// Número total de sitios (excluyendo carpetas).
    pub fn site_count(&self) -> usize {
        count_sites(&self.roots)
    }

    /// Carga `bookmarks.json`. Si el archivo no existe devuelve almacén vacío; si el JSON
    /// está corrupto guarda `<ruta>.bak`, emite `warn!` y arranca limpio.
    pub fn load(path: &Path) -> Self {
        if !path.exists() {
            return Self::new();
        }

        let raw = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(err) => {
                crate::warn!(
                    "No se pudo leer '{}': {err}. Arrancando marcadores vacíos.",
                    path.display()
                );
                return Self::new();
            }
        };

        match serde_json::from_str::<Vec<BookmarkNode>>(&raw) {
            Ok(mut roots) => {
                ensure_valid_ids(&mut roots);
                Self { roots }
            }
            Err(err) => {
                crate::warn!(
                    "JSON corrupto en marcadores '{}': {err}. Guardando respaldo .bak.",
                    path.display()
                );
                if let Err(bak_err) = paths::backup_corrupt_file(path) {
                    crate::warn!("No se pudo respaldar '{}': {bak_err}", path.display());
                }
                Self::new()
            }
        }
    }

    /// Guarda los marcadores de forma atómica en `path`.
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut json = serde_json::to_vec_pretty(&self.roots)?;
        json.push(b'\n');
        paths::write_atomic(path, &json)
    }

    pub fn contains_url(&self, raw_url: &str) -> bool {
        self.find_by_url(raw_url).is_some()
    }

    /// Busca el primer marcador cuya URL coincida (normalizando credenciales y `/` final).
    /// Devuelve `(nodo, parent_folder_id)`.
    pub fn find_by_url(&self, raw_url: &str) -> Option<(BookmarkNode, Option<String>)> {
        let needle = normalize_bookmark_url(raw_url);
        if needle.is_empty() {
            return None;
        }
        find_by_url_in(&self.roots, &needle, None)
    }

    pub fn find_by_id(&self, id: &str) -> Option<BookmarkNode> {
        find_by_id_in(&self.roots, id)
    }

    pub fn parent_of(&self, id: &str) -> Option<String> {
        find_parent_in(&self.roots, id, None).flatten()
    }

    /// Añade un marcador en `parent_folder_id` (o en la raíz si es `None`).
    pub fn add_bookmark(
        &mut self,
        title: &str,
        url: &str,
        parent_folder_id: Option<&str>,
    ) -> BookmarkNode {
        let node = BookmarkNode::site(title, url);
        if !place_node(&mut self.roots, node.clone(), parent_folder_id, None) {
            self.roots.push(node.clone());
        }
        node
    }

    /// Crea una carpeta en `parent_folder_id` (o en la raíz si es `None`).
    pub fn create_folder(&mut self, title: &str, parent_folder_id: Option<&str>) -> BookmarkNode {
        let node = BookmarkNode::folder(title, Vec::new());
        if !place_node(&mut self.roots, node.clone(), parent_folder_id, None) {
            self.roots.push(node.clone());
        }
        node
    }

    /// Actualiza título y/o URL de un nodo existente.
    pub fn update_node(&mut self, id: &str, title: Option<String>, url: Option<String>) -> bool {
        update_in(&mut self.roots, id, title.as_deref(), url.as_deref())
    }

    /// Elimina un marcador o carpeta (y sus hijos) por ID.
    pub fn remove(&mut self, id: &str) -> bool {
        remove_in(&mut self.roots, id)
    }

    /// Mueve `id` dentro de `target_folder_id` (`None` = raíz) en `target_index` (o al final).
    /// Impide mover una carpeta dentro de sí misma o de cualquiera de sus descendientes.
    pub fn move_node(
        &mut self,
        id: &str,
        target_folder_id: Option<&str>,
        mut target_index: Option<usize>,
    ) -> bool {
        if target_folder_id == Some(id) {
            return false;
        }
        if let Some(dst_idx) = target_index {
            if let Some(src_idx) = sibling_index(&self.roots, target_folder_id, id) {
                if src_idx < dst_idx {
                    target_index = Some(dst_idx - 1);
                }
            }
        }

        let mut working = self.roots.clone();
        let Some(node) = detach_in(&mut working, id) else {
            return false;
        };
        if let Some(folder_id) = target_folder_id {
            if subtree_contains(&node, folder_id) {
                return false;
            }
        }
        if !place_node(&mut working, node, target_folder_id, target_index) {
            return false;
        }
        self.roots = working;
        true
    }

    /// Reordena `src_id` justo antes de `dst_id` en el mismo padre de `dst_id`.
    pub fn reorder_before(&mut self, src_id: &str, dst_id: &str) -> bool {
        if src_id == dst_id {
            return false;
        }
        let Some((parent_id, dst_index)) = locate_parent_and_index(&self.roots, dst_id, None)
        else {
            return false;
        };
        self.move_node(src_id, parent_id.as_deref(), Some(dst_index))
    }

    /// Lista plana de todas las carpetas con su profundidad (para selectores de carpeta).
    pub fn folders(&self) -> Vec<FolderOption> {
        let mut out = Vec::new();
        collect_folders(&self.roots, 0, &mut out);
        out
    }

    /// Busca sitios (y opcionalmente carpetas) cuyo título o URL contenga todas las palabras.
    pub fn search(&self, query: &str, include_folders: bool, limit: usize) -> Vec<BookmarkMatch> {
        let needle = query.trim().to_ascii_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        let words: Vec<&str> = needle.split_whitespace().collect();
        if words.is_empty() {
            return Vec::new();
        }
        let bare_needle = strip_scheme_and_www(&needle);

        let mut out = Vec::new();
        search_walk(
            &self.roots,
            &[],
            &needle,
            &bare_needle,
            &words,
            include_folders,
            &mut out,
        );
        out.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.node.title.len().cmp(&b.node.title.len()))
        });
        if limit > 0 && out.len() > limit {
            out.truncate(limit);
        }
        out
    }

    /// Exporta todo el árbol de marcadores a HTML estándar de Netscape
    /// (`<!DOCTYPE NETSCAPE-Bookmark-file-1>`, compatible con Firefox y Chrome).
    pub fn export_netscape_html(&self) -> String {
        let mut out = String::from(
            "<!DOCTYPE NETSCAPE-Bookmark-file-1>\n\
             <!-- This is an automatically generated file.\n\
                  It will be read and overwritten.\n\
                  DO NOT EDIT! -->\n\
             <META HTTP-EQUIV=\"Content-Type\" CONTENT=\"text/html; charset=UTF-8\">\n\
             <TITLE>Bookmarks</TITLE>\n\
             <H1>Bookmarks</H1>\n\
             <DL><p>\n",
        );
        write_netscape_nodes(&self.roots, 1, &mut out);
        out.push_str("</DL><p>\n");
        out
    }

    /// Parsea un documento HTML de marcadores Netscape (Firefox, Chrome, Safari, Edge)
    /// y devuelve el árbol de `BookmarkNode` resultante.
    pub fn parse_netscape_html(html: &str) -> Vec<BookmarkNode> {
        parse_netscape_bookmarks_html(html)
    }

    /// Importa un archivo HTML de marcadores Netscape fusionando carpetas del mismo nombre
    /// y evitando duplicar URLs en el mismo nivel.
    pub fn import_netscape_html(&mut self, html: &str) -> BookmarkImportStats {
        let incoming = Self::parse_netscape_html(html);
        let mut stats = BookmarkImportStats::default();
        merge_nodes(&incoming, &mut self.roots, &mut stats);
        stats
    }
}

fn ensure_valid_ids(nodes: &mut [BookmarkNode]) {
    for node in nodes {
        if node.id.trim().is_empty() {
            node.id = generate_uuid();
        }
        if let Some(ref mut kids) = node.children {
            ensure_valid_ids(kids);
        }
    }
}

fn count_sites(nodes: &[BookmarkNode]) -> usize {
    nodes
        .iter()
        .map(|n| {
            if n.is_folder() {
                count_sites(n.children.as_deref().unwrap_or(&[]))
            } else {
                1
            }
        })
        .sum()
}

pub(crate) fn normalize_bookmark_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if let Ok(mut parsed) = url::Url::parse(trimmed) {
        let _ = parsed.set_username("");
        let _ = parsed.set_password(None);
        if parsed.path().is_empty() {
            parsed.set_path("/");
        }
        return parsed.to_string();
    }
    trimmed.to_string()
}

fn strip_scheme_and_www(raw: &str) -> String {
    let lower = raw.trim().to_ascii_lowercase();
    let without_scheme = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .unwrap_or(&lower);
    without_scheme
        .strip_prefix("www.")
        .unwrap_or(without_scheme)
        .to_string()
}

fn find_by_url_in(
    nodes: &[BookmarkNode],
    needle: &str,
    parent: Option<&str>,
) -> Option<(BookmarkNode, Option<String>)> {
    for node in nodes {
        if let Some(ref u) = node.url {
            if normalize_bookmark_url(u) == needle {
                return Some((node.clone(), parent.map(String::from)));
            }
        }
        if let Some(ref kids) = node.children {
            if let Some(found) = find_by_url_in(kids, needle, Some(&node.id)) {
                return Some(found);
            }
        }
    }
    None
}

fn find_by_id_in(nodes: &[BookmarkNode], id: &str) -> Option<BookmarkNode> {
    for node in nodes {
        if node.id == id {
            return Some(node.clone());
        }
        if let Some(ref kids) = node.children {
            if let Some(found) = find_by_id_in(kids, id) {
                return Some(found);
            }
        }
    }
    None
}

fn find_parent_in(
    nodes: &[BookmarkNode],
    id: &str,
    current_parent: Option<&str>,
) -> Option<Option<String>> {
    for node in nodes {
        if node.id == id {
            return Some(current_parent.map(String::from));
        }
        if let Some(ref kids) = node.children {
            if let Some(found) = find_parent_in(kids, id, Some(&node.id)) {
                return Some(found);
            }
        }
    }
    None
}

fn locate_parent_and_index(
    nodes: &[BookmarkNode],
    target_id: &str,
    current_parent: Option<&str>,
) -> Option<(Option<String>, usize)> {
    for (idx, node) in nodes.iter().enumerate() {
        if node.id == target_id {
            return Some((current_parent.map(String::from), idx));
        }
        if let Some(ref kids) = node.children {
            if let Some(found) = locate_parent_and_index(kids, target_id, Some(&node.id)) {
                return Some(found);
            }
        }
    }
    None
}

fn sibling_index(
    nodes: &[BookmarkNode],
    parent_folder_id: Option<&str>,
    child_id: &str,
) -> Option<usize> {
    match parent_folder_id {
        None => nodes.iter().position(|n| n.id == child_id),
        Some(folder_id) => {
            for node in nodes {
                if node.id == folder_id {
                    return node
                        .children
                        .as_deref()
                        .and_then(|kids| kids.iter().position(|c| c.id == child_id));
                }
                if let Some(ref kids) = node.children {
                    if let Some(found) = sibling_index(kids, Some(folder_id), child_id) {
                        return Some(found);
                    }
                }
            }
            None
        }
    }
}

fn subtree_contains(node: &BookmarkNode, id: &str) -> bool {
    if node.id == id {
        return true;
    }
    node.children
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .any(|c| subtree_contains(c, id))
}

fn detach_in(nodes: &mut Vec<BookmarkNode>, id: &str) -> Option<BookmarkNode> {
    if let Some(pos) = nodes.iter().position(|n| n.id == id) {
        return Some(nodes.remove(pos));
    }
    for node in nodes.iter_mut() {
        if let Some(ref mut kids) = node.children {
            if let Some(found) = detach_in(kids, id) {
                return Some(found);
            }
        }
    }
    None
}

fn place_node(
    nodes: &mut Vec<BookmarkNode>,
    item: BookmarkNode,
    folder_id: Option<&str>,
    index: Option<usize>,
) -> bool {
    match folder_id {
        None => {
            let at = index.unwrap_or(nodes.len()).min(nodes.len());
            nodes.insert(at, item);
            true
        }
        Some(target) => {
            for node in nodes.iter_mut() {
                if node.id == target && node.is_folder() {
                    let kids = node.children.get_or_insert_with(Vec::new);
                    let at = index.unwrap_or(kids.len()).min(kids.len());
                    kids.insert(at, item);
                    return true;
                }
                if let Some(ref mut kids) = node.children {
                    if place_node(kids, item.clone(), Some(target), index) {
                        return true;
                    }
                }
            }
            false
        }
    }
}

fn update_in(nodes: &mut [BookmarkNode], id: &str, title: Option<&str>, url: Option<&str>) -> bool {
    for node in nodes.iter_mut() {
        if node.id == id {
            if let Some(new_title) = title {
                let trimmed = new_title.trim();
                if !trimmed.is_empty() || !node.is_folder() {
                    node.title = trimmed.to_string();
                }
            }
            if let Some(new_url) = url {
                if !node.is_folder() && !new_url.trim().is_empty() {
                    node.url = Some(sanitize_url_for_ui(new_url.trim()));
                }
            }
            return true;
        }
        if let Some(ref mut kids) = node.children {
            if update_in(kids, id, title, url) {
                return true;
            }
        }
    }
    false
}

fn remove_in(nodes: &mut Vec<BookmarkNode>, id: &str) -> bool {
    if let Some(pos) = nodes.iter().position(|n| n.id == id) {
        nodes.remove(pos);
        return true;
    }
    for node in nodes.iter_mut() {
        if let Some(ref mut kids) = node.children {
            if remove_in(kids, id) {
                return true;
            }
        }
    }
    false
}

fn collect_folders(nodes: &[BookmarkNode], depth: usize, out: &mut Vec<FolderOption>) {
    for node in nodes {
        if node.is_folder() {
            out.push(FolderOption {
                id: node.id.clone(),
                title: node.display_label(),
                depth,
            });
            if let Some(ref kids) = node.children {
                collect_folders(kids, depth + 1, out);
            }
        }
    }
}

fn search_walk(
    nodes: &[BookmarkNode],
    path: &[String],
    needle: &str,
    bare_needle: &str,
    words: &[&str],
    include_folders: bool,
    out: &mut Vec<BookmarkMatch>,
) {
    for node in nodes {
        let title_lower = node.title.to_lowercase();
        let addr_bare = node
            .url
            .as_deref()
            .map(strip_scheme_and_www)
            .unwrap_or_default();
        let hay = format!("{title_lower} {addr_bare}");

        if (!node.is_folder() || include_folders) && words.iter().all(|w| hay.contains(w)) {
            let score = if !bare_needle.is_empty() && addr_bare.starts_with(bare_needle) {
                3
            } else if title_lower.starts_with(needle)
                || title_lower
                    .split_whitespace()
                    .any(|w| w.starts_with(words[0]))
            {
                2
            } else {
                1
            };
            out.push(BookmarkMatch {
                node: node.clone(),
                folder_path: path.to_vec(),
                score,
            });
        }

        if let Some(ref kids) = node.children {
            let mut next_path = path.to_vec();
            next_path.push(node.display_label());
            search_walk(
                kids,
                &next_path,
                needle,
                bare_needle,
                words,
                include_folders,
                out,
            );
        }
    }
}

fn merge_nodes(
    incoming: &[BookmarkNode],
    target: &mut Vec<BookmarkNode>,
    stats: &mut BookmarkImportStats,
) {
    let mut folder_indices: HashMap<String, usize> = HashMap::new();
    let mut existing_urls: HashSet<String> = HashSet::new();

    for (idx, node) in target.iter().enumerate() {
        if node.is_folder() {
            folder_indices
                .entry(node.title.to_lowercase())
                .or_insert(idx);
        } else if let Some(ref u) = node.url {
            existing_urls.insert(normalize_bookmark_url(u));
        }
    }

    for node in incoming {
        if node.is_folder() {
            let key = node.title.to_lowercase();
            if let Some(&idx) = folder_indices.get(&key) {
                let kids = target[idx].children.get_or_insert_with(Vec::new);
                merge_nodes(node.children.as_deref().unwrap_or(&[]), kids, stats);
            } else {
                let mut cloned = node.clone();
                cloned.id = generate_uuid();
                assign_fresh_ids(&mut cloned);
                stats.added += count_sites(&[cloned.clone()]);
                folder_indices.insert(key, target.len());
                target.push(cloned);
            }
        } else if let Some(ref u) = node.url {
            let norm = normalize_bookmark_url(u);
            if existing_urls.contains(&norm) {
                stats.skipped += 1;
            } else {
                existing_urls.insert(norm);
                let mut cloned = node.clone();
                cloned.id = generate_uuid();
                target.push(cloned);
                stats.added += 1;
            }
        }
    }
}

fn assign_fresh_ids(node: &mut BookmarkNode) {
    node.id = generate_uuid();
    if let Some(ref mut kids) = node.children {
        for k in kids.iter_mut() {
            assign_fresh_ids(k);
        }
    }
}

// ---------------------------------------------------------------------------
// Exportación e importación HTML de marcadores Netscape
// ---------------------------------------------------------------------------

fn write_netscape_nodes(nodes: &[BookmarkNode], indent: usize, out: &mut String) {
    let pad = "    ".repeat(indent);
    for node in nodes {
        if node.is_folder() {
            out.push_str(&pad);
            out.push_str("<DT><H3>");
            out.push_str(&escape_html(&node.display_label()));
            out.push_str("</H3>\n");
            out.push_str(&pad);
            out.push_str("<DL><p>\n");
            if let Some(ref kids) = node.children {
                write_netscape_nodes(kids, indent + 1, out);
            }
            out.push_str(&pad);
            out.push_str("</DL><p>\n");
        } else if let Some(ref url) = node.url {
            out.push_str(&pad);
            out.push_str("<DT><A HREF=\"");
            out.push_str(&escape_html(url));
            out.push_str("\">");
            out.push_str(&escape_html(&node.display_label()));
            out.push_str("</A>\n");
        }
    }
}

fn escape_html(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn decode_html_entities(raw: &str) -> String {
    if !raw.contains('&') {
        return raw.to_string();
    }
    let bytes = raw.as_bytes();
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            if let Some(semi_rel) = bytes[i + 1..].iter().position(|&b| b == b';') {
                if semi_rel <= 16 {
                    let entity = &raw[i + 1..i + 1 + semi_rel];
                    if let Some(decoded) = decode_single_entity(entity) {
                        out.push_str(&decoded);
                        i += semi_rel + 2;
                        continue;
                    }
                }
            }
        }
        if let Some(ch) = raw[i..].chars().next() {
            out.push(ch);
            i += ch.len_utf8();
        } else {
            break;
        }
    }
    out
}

fn decode_single_entity(entity: &str) -> Option<String> {
    if let Some(num) = entity.strip_prefix('#') {
        let code = if let Some(hex) = num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
            u32::from_str_radix(hex, 16).ok()?
        } else {
            num.parse::<u32>().ok()?
        };
        return char::from_u32(code).map(|c| c.to_string());
    }
    match entity.to_ascii_lowercase().as_str() {
        "amp" => Some("&".to_string()),
        "lt" => Some("<".to_string()),
        "gt" => Some(">".to_string()),
        "quot" => Some("\"".to_string()),
        "apos" => Some("'".to_string()),
        "nbsp" => Some(" ".to_string()),
        _ => None,
    }
}

fn strip_inner_tags(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut in_tag = false;
    for ch in raw.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    decode_html_entities(&out).trim().to_string()
}

fn extract_html_attr(attrs: &str, name: &str) -> Option<String> {
    let lower = attrs.to_ascii_lowercase();
    let needle = name.to_ascii_lowercase();
    let mut offset = 0;

    while let Some(pos) = lower[offset..].find(&needle) {
        let abs = offset + pos;
        let before_ok = abs == 0
            || lower.as_bytes()[abs - 1].is_ascii_whitespace()
            || lower.as_bytes()[abs - 1] == b'<';
        let after = abs + needle.len();
        if before_ok {
            let rest = &attrs[after..];
            let trimmed_ws = rest.trim_start();
            if let Some(after_eq) = trimmed_ws.strip_prefix('=') {
                let val_start = after_eq.trim_start();
                if let Some(quoted) = val_start.strip_prefix('"') {
                    let end = quoted.find('"').unwrap_or(quoted.len());
                    return Some(decode_html_entities(&quoted[..end]));
                }
                if let Some(quoted) = val_start.strip_prefix('\'') {
                    let end = quoted.find('\'').unwrap_or(quoted.len());
                    return Some(decode_html_entities(&quoted[..end]));
                }
                let end = val_start
                    .find(|c: char| c.is_ascii_whitespace() || c == '>')
                    .unwrap_or(val_start.len());
                return Some(decode_html_entities(&val_start[..end]));
            }
        }
        offset = after;
    }
    None
}

fn parse_netscape_bookmarks_html(html: &str) -> Vec<BookmarkNode> {
    struct StackFrame {
        title: Option<String>,
        nodes: Vec<BookmarkNode>,
    }

    let mut stack: Vec<StackFrame> = Vec::new();
    let mut root: Vec<BookmarkNode> = Vec::new();
    let mut pending_heading: Option<String> = None;
    let mut pending_toolbar = false;

    let append_node =
        |node: BookmarkNode, stack: &mut Vec<StackFrame>, root: &mut Vec<BookmarkNode>| {
            if let Some(top) = stack.last_mut() {
                top.nodes.push(node);
            } else {
                root.push(node);
            }
        };

    let bytes = html.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let Some(lt_rel) = bytes[i..].iter().position(|&b| b == b'<') else {
            break;
        };
        let tag_start = i + lt_rel;
        let Some(gt_rel) = bytes[tag_start + 1..].iter().position(|&b| b == b'>') else {
            break;
        };
        let tag_end = tag_start + 1 + gt_rel;
        let tag_content = html[tag_start + 1..tag_end].trim();
        i = tag_end + 1;

        if tag_content.is_empty() || tag_content.starts_with('!') || tag_content.starts_with('?') {
            continue;
        }

        let is_closing = tag_content.starts_with('/');
        let name_part = if is_closing {
            tag_content[1..].trim_start()
        } else {
            tag_content
        };
        let name_end = name_part
            .find(|c: char| c.is_ascii_whitespace() || c == '/')
            .unwrap_or(name_part.len());
        let tag_name = name_part[..name_end].to_ascii_uppercase();
        let attrs = &name_part[name_end..];

        match (is_closing, tag_name.as_str()) {
            (false, "H3") => {
                if let Some((inner, next_pos)) = read_until_close_tag(html, i, "h3") {
                    pending_heading = Some(strip_inner_tags(inner));
                    pending_toolbar = extract_html_attr(attrs, "PERSONAL_TOOLBAR_FOLDER")
                        .map(|v| v.eq_ignore_ascii_case("true"))
                        .unwrap_or(false);
                    i = next_pos;
                }
            }
            (false, "A") => {
                if let Some((inner, next_pos)) = read_until_close_tag(html, i, "a") {
                    let title = strip_inner_tags(inner);
                    if let Some(href) = extract_html_attr(attrs, "HREF") {
                        if let Ok(parsed) = url::Url::parse(href.trim()) {
                            let scheme = parsed.scheme();
                            if scheme == "http" || scheme == "https" {
                                let label = if title.is_empty() {
                                    parsed.host_str().unwrap_or(href.trim()).to_string()
                                } else {
                                    title
                                };
                                append_node(
                                    BookmarkNode::site(label, parsed.as_str()),
                                    &mut stack,
                                    &mut root,
                                );
                            }
                        }
                    }
                    i = next_pos;
                }
            }
            (false, "DL") => {
                let flatten = stack.len() >= 64
                    || (pending_heading.is_none() && stack.is_empty() && root.is_empty())
                    || (pending_toolbar && stack.len() <= 1);
                if flatten {
                    stack.push(StackFrame {
                        title: None,
                        nodes: Vec::new(),
                    });
                } else {
                    let folder_title = pending_heading
                        .take()
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "Carpeta".to_string());
                    stack.push(StackFrame {
                        title: Some(folder_title),
                        nodes: Vec::new(),
                    });
                }
                pending_heading = None;
                pending_toolbar = false;
            }
            (true, "DL") => {
                if let Some(frame) = stack.pop() {
                    if let Some(title) = frame.title {
                        append_node(
                            BookmarkNode::folder(title, frame.nodes),
                            &mut stack,
                            &mut root,
                        );
                    } else if let Some(parent) = stack.last_mut() {
                        parent.nodes.extend(frame.nodes);
                    } else {
                        root.extend(frame.nodes);
                    }
                }
            }
            _ => {}
        }
    }

    while let Some(frame) = stack.pop() {
        if let Some(title) = frame.title {
            append_node(
                BookmarkNode::folder(title, frame.nodes),
                &mut stack,
                &mut root,
            );
        } else if let Some(parent) = stack.last_mut() {
            parent.nodes.extend(frame.nodes);
        } else {
            root.extend(frame.nodes);
        }
    }

    root
}

fn read_until_close_tag<'a>(
    html: &'a str,
    start: usize,
    tag_lower: &str,
) -> Option<(&'a str, usize)> {
    let rest = &html[start..];
    let lower = rest.to_ascii_lowercase();
    let needle = format!("</{tag_lower}");
    let mut search_from = 0;
    while let Some(pos) = lower[search_from..].find(&needle) {
        let abs = search_from + pos;
        let after_name = abs + needle.len();
        let after_slice = &lower[after_name..];
        let trimmed = after_slice.trim_start();
        if let Some(gt) = trimmed.find('>') {
            if gt == 0 {
                let ws_len = after_slice.len() - trimmed.len();
                let end_pos = start + after_name + ws_len + 1;
                return Some((&rest[..abs], end_pos));
            }
        }
        search_from = after_name;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::DotPaths;

    static TEST_SEQ: AtomicU64 = AtomicU64::new(1);

    fn temp_bookmarks_path(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let n = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "search-bookmarks-test-{tag}-{}-{n}",
            std::process::id()
        ));
        let sp = DotPaths::with_base(&root);
        sp.ensure_dirs().expect("ensure_dirs");
        (root, sp.bookmarks_file())
    }

    #[test]
    fn tree_crud_and_cycle_prevention() {
        let mut store = BookmarkStore::new();
        let rust_folder = store.create_folder("Rust", None);
        let docs_sub = store.create_folder("Docs", Some(&rust_folder.id));
        let book = store.add_bookmark(
            "The Rust Book",
            "https://doc.rust-lang.org/book/",
            Some(&docs_sub.id),
        );
        let root_site = store.add_bookmark("Example", "https://example.com/", None);

        assert_eq!(store.site_count(), 2);
        assert!(store.contains_url("https://doc.rust-lang.org/book/"));
        assert_eq!(
            store.parent_of(&book.id).as_deref(),
            Some(docs_sub.id.as_str())
        );

        // No debe permitir mover una carpeta dentro de su propia subcarpeta.
        assert!(!store.move_node(&rust_folder.id, Some(&docs_sub.id), None));
        assert!(!store.move_node(&rust_folder.id, Some(&rust_folder.id), None));

        // Reordenar Example antes de Rust en la raíz.
        assert!(store.reorder_before(&root_site.id, &rust_folder.id));
        assert_eq!(store.roots()[0].id, root_site.id);
        assert_eq!(store.roots()[1].id, rust_folder.id);

        // Renombrar y eliminar.
        assert!(store.update_node(
            &book.id,
            Some("Rust Book 2026".to_string()),
            Some("https://doc.rust-lang.org/book/ch01.html".to_string()),
        ));
        let updated = store.find_by_id(&book.id).expect("updated bookmark");
        assert_eq!(updated.title, "Rust Book 2026");
        assert!(store.remove(&rust_folder.id));
        assert_eq!(store.site_count(), 1);
    }

    #[test]
    fn netscape_html_export_and_import_roundtrip() {
        let mut orig = BookmarkStore::new();
        orig.add_bookmark("Example & Friends", "https://example.com/?a=1&b=2", None);
        let folder = orig.create_folder("Desarrollo <Rust>", None);
        orig.add_bookmark("Docs.rs", "https://docs.rs/cef/", Some(&folder.id));
        let sub = orig.create_folder("GNOME", Some(&folder.id));
        orig.add_bookmark("GTK4 Docs", "https://docs.gtk.org/gtk4/", Some(&sub.id));

        let html = orig.export_netscape_html();
        assert!(html.starts_with("<!DOCTYPE NETSCAPE-Bookmark-file-1>"));
        assert!(html.contains("Example &amp; Friends"));
        assert!(html.contains("Desarrollo &lt;Rust&gt;"));

        let mut imported = BookmarkStore::new();
        let stats = imported.import_netscape_html(&html);
        assert_eq!(stats.added, 3);
        assert_eq!(stats.skipped, 0);
        assert_eq!(imported.site_count(), 3);
        assert!(imported.contains_url("https://example.com/?a=1&b=2"));
        assert!(imported.contains_url("https://docs.gtk.org/gtk4/"));

        // Re-importar el mismo HTML no debe duplicar entradas.
        let stats2 = imported.import_netscape_html(&html);
        assert_eq!(stats2.added, 0);
        assert_eq!(stats2.skipped, 3);
        assert_eq!(imported.site_count(), 3);
    }

    #[test]
    fn parse_firefox_and_chrome_netscape_html_variants() {
        let sample = r#"
<!DOCTYPE NETSCAPE-Bookmark-file-1>
<META HTTP-EQUIV="Content-Type" CONTENT="text/html; charset=UTF-8">
<TITLE>Bookmarks</TITLE>
<H1>Bookmarks Menu</H1>
<DL><p>
    <DT><H3 PERSONAL_TOOLBAR_FOLDER="true">Bookmarks Bar</H3>
    <DL><p>
        <DT><A HREF="https://search.example/home" ADD_DATE="1700000000">Search &amp; News</A>
        <DT><A HREF="javascript:alert(1)">Bookmarklet ignorado</A>
    </DL><p>
    <DT><H3 FOLDED>Lecturas &#39;26</H3>
    <DL><p>
        <DT><a href='https://lwn.net/'>LWN &#x2014; Linux</a>
    </DL><p>
</DL><p>
"#;
        let nodes = BookmarkStore::parse_netscape_html(sample);
        assert_eq!(nodes.len(), 2);
        // La barra de marcadores se aplana a la raíz.
        assert_eq!(nodes[0].title, "Search & News");
        assert_eq!(nodes[0].url.as_deref(), Some("https://search.example/home"));
        assert!(nodes[1].is_folder());
        assert_eq!(nodes[1].title, "Lecturas '26");
        let kids = nodes[1].children.as_ref().expect("folder children");
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].title, "LWN \u{2014} Linux");
    }

    #[test]
    fn json_persistence_and_corrupt_bak_recovery() {
        let (root, file) = temp_bookmarks_path("persist");
        let mut store = BookmarkStore::new();
        let f = store.create_folder("Favoritos", None);
        store.add_bookmark("Rust", "https://www.rust-lang.org/", Some(&f.id));
        store.save(&file).expect("save");

        let loaded = BookmarkStore::load(&file);
        assert_eq!(loaded.site_count(), 1);
        assert!(loaded.contains_url("https://www.rust-lang.org/"));

        paths::write_atomic(&file, b"[corrupt").expect("write corrupt");
        let recovered = BookmarkStore::load(&file);
        assert!(recovered.is_empty());
        assert!(file.with_extension("json.bak").exists());

        let _ = fs::remove_dir_all(&root);
    }
}
