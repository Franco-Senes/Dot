//! Curtain: generación pura en Rust de selectores CSS estables a partir de una instantánea
//! mínima del DOM y persistencia atómica por dominio en `$XDG_DATA_HOME/search/curtain.json`
//! (`Curtain.swift` + `Hidden.swift`).

use crate::domain::site_host;
use crate::paths;
use crate::shield::js_string_literal;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Atributos semánticos estables priorizados al generar selectores CSS.
pub const STABLE_HOOK_ATTRS: &[&str] = &[
    "data-testid",
    "data-test",
    "data-qa",
    "data-cy",
    "aria-label",
    "name",
    "role",
];

/// Prefijos típicos de clases generadas por bundlers/CSS-in-JS que cambian entre despliegues.
const GENERATED_CLASS_PREFIXES: &[&str] = &[
    "css-", "sc-", "jsx-", "emotion-", "svelte-", "style-", "styles-", "styled-", "chakra-", "mui-",
];

fn default_true() -> bool {
    true
}

/// Regla persistida para ocultar un elemento en un sitio (`PORTING.md` §4 `curtain.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Veil {
    pub selector: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub date: f64,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Estadísticas de importación de reglas Curtain desde JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CurtainImportStats {
    pub added: usize,
    pub skipped: usize,
}

/// Almacén persistente de reglas Curtain agrupadas por dominio sin `www.`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CurtainStore {
    pub by_host: BTreeMap<String, Vec<Veil>>,
}

impl CurtainStore {
    /// Carga `curtain.json` desde disco. Ante JSON corrupto guarda `<archivo>.bak` y arranca limpio.
    pub fn load(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }
        let raw = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(err) => {
                crate::warn!(
                    "No se pudo leer Curtain '{}': {err}. Arrancando vacío.",
                    path.display()
                );
                return Self::default();
            }
        };
        match serde_json::from_str::<Self>(&raw) {
            Ok(mut store) => {
                store.normalize();
                store
            }
            Err(err) => {
                crate::warn!(
                    "JSON corrupto en Curtain '{}': {err}. Guardando respaldo .bak.",
                    path.display()
                );
                if let Err(bak_err) = paths::backup_corrupt_file(path) {
                    crate::warn!("No se pudo crear respaldo .bak de Curtain: {bak_err}");
                }
                Self::default()
            }
        }
    }

    /// Persiste `curtain.json` de forma atómica (`tmp` + `sync_all` + `rename`).
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut normalized = self.clone();
        normalized.normalize();
        let mut json = serde_json::to_vec_pretty(&normalized)?;
        json.push(b'\n');
        paths::write_atomic(path, &json)
    }

    fn normalize(&mut self) {
        let mut cleaned: BTreeMap<String, Vec<Veil>> = BTreeMap::new();
        for (raw_host, veils) in std::mem::take(&mut self.by_host) {
            let Some(host) = site_host(&raw_host) else {
                continue;
            };
            let entry = cleaned.entry(host).or_default();
            for mut veil in veils {
                veil.selector = veil.selector.trim().to_string();
                if veil.selector.is_empty() {
                    continue;
                }
                if veil.label.trim().is_empty() {
                    veil.label = veil.selector.clone();
                }
                if let Some(ref note) = veil.note {
                    if note.trim().is_empty() {
                        veil.note = None;
                    }
                }
                if !entry.iter().any(|v| v.selector == veil.selector) {
                    entry.push(veil);
                }
            }
        }
        cleaned.retain(|_, v| !v.is_empty());
        self.by_host = cleaned;
    }

    /// Devuelve el host normalizado (sin `www.`) de una URL web para indexar reglas de Curtain.
    pub fn host_for_url(url: &str) -> Option<String> {
        site_host(url)
    }

    pub fn veils(&self, host: &str) -> &[Veil] {
        let Some(norm) = site_host(host) else {
            return &[];
        };
        self.by_host.get(&norm).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn hosts(&self) -> Vec<String> {
        self.by_host.keys().cloned().collect()
    }

    pub fn total_rules(&self) -> usize {
        self.by_host.values().map(Vec::len).sum()
    }

    /// Añade una regla de ocultación para `host`. Devuelve `false` si ya existía o era inválida.
    pub fn hide(
        &mut self,
        host: &str,
        selector: String,
        label: String,
        note: Option<String>,
        date: f64,
    ) -> bool {
        let Some(norm_host) = site_host(host) else {
            return false;
        };
        let sel = selector.trim().to_string();
        if sel.is_empty() {
            return false;
        }
        let lbl = {
            let t = label.trim();
            if t.is_empty() {
                sel.clone()
            } else {
                t.to_string()
            }
        };
        let clean_note = note.and_then(|n| {
            let t = n.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        });

        let list = self.by_host.entry(norm_host).or_default();
        if list.iter().any(|v| v.selector == sel) {
            return false;
        }
        list.push(Veil {
            selector: sel,
            label: lbl,
            note: clean_note,
            date,
            enabled: true,
        });
        true
    }

    /// Activa o desactiva una regla existente en `host` sin eliminarla. Devuelve `true` si cambió.
    pub fn set_enabled(&mut self, host: &str, selector: &str, enabled: bool) -> bool {
        let Some(norm_host) = site_host(host) else {
            return false;
        };
        let Some(list) = self.by_host.get_mut(&norm_host) else {
            return false;
        };
        let Some(veil) = list.iter_mut().find(|v| v.selector == selector) else {
            return false;
        };
        if veil.enabled == enabled {
            return false;
        }
        veil.enabled = enabled;
        true
    }

    /// Restaura una regla concreta en `host`. Devuelve `true` si existía y se eliminó.
    pub fn restore(&mut self, host: &str, selector: &str) -> bool {
        let Some(norm_host) = site_host(host) else {
            return false;
        };
        let Some(list) = self.by_host.get_mut(&norm_host) else {
            return false;
        };
        let before = list.len();
        list.retain(|v| v.selector != selector);
        let removed = list.len() != before;
        if list.is_empty() {
            self.by_host.remove(&norm_host);
        }
        removed
    }

    /// Deshace la última ocultación realizada en `host`.
    pub fn undo(&mut self, host: &str) -> Option<Veil> {
        let norm_host = site_host(host)?;
        let list = self.by_host.get_mut(&norm_host)?;
        let last = list.pop()?;
        if list.is_empty() {
            self.by_host.remove(&norm_host);
        }
        Some(last)
    }

    /// Restaura todas las reglas de un sitio concreto. Devuelve cuántas se eliminaron.
    pub fn restore_all(&mut self, host: &str) -> usize {
        let Some(norm_host) = site_host(host) else {
            return 0;
        };
        self.by_host
            .remove(&norm_host)
            .map(|v| v.len())
            .unwrap_or(0)
    }

    /// Elimina todas las reglas de todos los sitios.
    pub fn clear_all(&mut self) -> usize {
        let total = self.total_rules();
        self.by_host.clear();
        total
    }

    /// Construye la hoja CSS para `host`, opcionalmente omitiendo `spared_selector` para previsualizar.
    /// Cada selector va en su propia regla para aislar errores de parseo CSS (`Curtain.swift`).
    pub fn css_for_host(&self, host: &str, spared_selector: Option<&str>) -> String {
        self.veils(host)
            .iter()
            .filter(|v| v.enabled && Some(v.selector.as_str()) != spared_selector)
            .map(|v| format!("{} {{ display: none !important; }}", v.selector))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Exporta todas las reglas normalizadas como JSON legible.
    pub fn export_json(&self) -> Result<String> {
        let mut normalized = self.clone();
        normalized.normalize();
        let mut out = serde_json::to_string_pretty(&normalized)?;
        out.push('\n');
        Ok(out)
    }

    /// Importa reglas desde JSON y las fusiona sin duplicar `(host, selector)`.
    pub fn import_json(&mut self, raw: &str) -> Result<CurtainImportStats> {
        let mut incoming: Self = serde_json::from_str(raw)?;
        incoming.normalize();
        let mut stats = CurtainImportStats::default();
        for (host, veils) in incoming.by_host {
            let entry = self.by_host.entry(host).or_default();
            for veil in veils {
                if entry.iter().any(|v| v.selector == veil.selector) {
                    stats.skipped += 1;
                } else {
                    entry.push(veil);
                    stats.added += 1;
                }
            }
        }
        self.normalize();
        Ok(stats)
    }
}

/// Atributo recogido por el colector mínimo JS para evaluar unicidad en Rust.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomAttrSnapshot {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub unique_with_tag: bool,
}

/// Información estructural de un nodo DOM (del objetivo hacia sus ancestros).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomNodeSnapshot {
    pub tag: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub id_unique: bool,
    #[serde(default)]
    pub classes: Vec<String>,
    /// Subconjunto de `classes` donde `tag.<clase>` identifica de forma única a este nodo.
    #[serde(default)]
    pub unique_classes: Vec<String>,
    /// Indica si la combinación de las clases estables de este nodo con su etiqueta es única.
    #[serde(default)]
    pub classes_combo_unique: bool,
    #[serde(default)]
    pub attrs: Vec<DomAttrSnapshot>,
    /// Índice 1-based respecto a hermanos con la misma etiqueta (`:nth-of-type`).
    #[serde(default = "default_one")]
    pub nth_of_type: usize,
    /// Total de hermanos con la misma etiqueta bajo el mismo padre.
    #[serde(default = "default_one")]
    pub same_tag_siblings: usize,
}

fn default_one() -> usize {
    1
}

/// Instantánea mínima de ancestros DOM enviada por `curtain.js` al hacer clic sobre un elemento.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementPickSnapshot {
    /// Cadena de nodos empezando por el elemento seleccionado (`ancestry[0]`) hasta `body`.
    pub ancestry: Vec<DomNodeSnapshot>,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub note: Option<String>,
}

/// Escapa un identificador CSS conforme a CSSOM `CSS.escape()` en Rust puro.
pub fn css_escape_ident(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 4);
    let chars: Vec<char> = raw.chars().collect();
    for (idx, &ch) in chars.iter().enumerate() {
        let code = ch as u32;
        if code == 0 {
            out.push('\u{FFFD}');
            continue;
        }
        let is_control = (0x01..=0x1F).contains(&code) || code == 0x7F;
        let is_leading_digit = idx == 0 && ch.is_ascii_digit();
        let is_digit_after_dash = idx == 1 && chars[0] == '-' && ch.is_ascii_digit();
        if is_control || is_leading_digit || is_digit_after_dash {
            use std::fmt::Write;
            let _ = write!(&mut out, "\\{code:x} ");
            continue;
        }
        if idx == 0 && chars.len() == 1 && ch == '-' {
            out.push_str("\\-");
            continue;
        }
        if code >= 0x80 || ch == '-' || ch == '_' || ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else {
            out.push('\\');
            out.push(ch);
        }
    }
    out
}

/// Escapa el valor de un atributo entre comillas dobles `[attr="valor"]`.
pub fn css_escape_attr_value(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 4);
    for ch in raw.chars() {
        let code = ch as u32;
        if ch == '\\' || ch == '"' {
            out.push('\\');
            out.push(ch);
        } else if (0x01..=0x1F).contains(&code) || code == 0x7F {
            use std::fmt::Write;
            let _ = write!(&mut out, "\\{code:x} ");
        } else {
            out.push(ch);
        }
    }
    out
}

/// Determina si una clase CSS parece semántica/estable y no un artefacto generado por build/CSS-in-JS.
pub fn is_steady_class(cls: &str) -> bool {
    let bytes = cls.as_bytes();
    if bytes.len() < 3 || bytes.len() > 30 {
        return false;
    }
    if !bytes[0].is_ascii_alphabetic() {
        return false;
    }
    if !bytes
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
    {
        return false;
    }
    // Rechazar secuencias de 3 o más dígitos seguidos (hashes / IDs numéricos).
    let mut digit_run = 0usize;
    for &b in bytes {
        if b.is_ascii_digit() {
            digit_run += 1;
            if digit_run >= 3 {
                return false;
            }
        } else {
            digit_run = 0;
        }
    }

    let lower = cls.to_ascii_lowercase();
    if GENERATED_CLASS_PREFIXES
        .iter()
        .any(|p| lower.starts_with(p))
    {
        return false;
    }

    // Rechazar sufijos de CSS Modules tipo `Card_banner__VTyoS` o `banner_a1b2c3d`.
    if let Some((_, suffix)) = cls.rsplit_once("__") {
        if looks_like_hash_token(suffix) {
            return false;
        }
    }
    if let Some((_, suffix)) = cls.rsplit_once('_') {
        if suffix.len() >= 5 && looks_like_hash_token(suffix) {
            return false;
        }
    }

    true
}

fn looks_like_hash_token(token: &str) -> bool {
    if token.len() < 5 {
        return false;
    }
    let has_upper = token.bytes().any(|b| b.is_ascii_uppercase());
    let has_lower = token.bytes().any(|b| b.is_ascii_lowercase());
    let has_digit = token.bytes().any(|b| b.is_ascii_digit());
    let all_hex = token.bytes().all(|b| b.is_ascii_hexdigit());
    (has_upper && has_lower) || (has_digit && (has_upper || has_lower)) || (all_hex && has_digit)
}

/// Determina si un atributo `id` es semántico/estable frente a IDs aleatorios o autogenerados.
pub fn is_steady_id(id: &str) -> bool {
    let trimmed = id.trim();
    if trimmed.is_empty() || trimmed.len() > 80 {
        return false;
    }
    if trimmed.starts_with(':') && trimmed.ends_with(':') {
        // React useId (`:r0:`, `:R1a:`)
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    if (lower.starts_with("ember")
        || lower.starts_with("radix-")
        || lower.starts_with("headlessui-")
        || lower.starts_with("react-"))
        && lower.bytes().any(|b| b.is_ascii_digit())
    {
        return false;
    }
    // Rechazar UUIDs v4 / hashes largos hexadecimales o numéricos puros.
    let hex_only_len = trimmed
        .bytes()
        .filter(|b| b.is_ascii_hexdigit() || *b == b'-')
        .count();
    let digit_count = trimmed.bytes().filter(|b| b.is_ascii_digit()).count();
    if trimmed.len() >= 16 && hex_only_len == trimmed.len() && digit_count >= 4 {
        return false;
    }
    if trimmed.bytes().all(|b| b.is_ascii_digit()) && trimmed.len() >= 4 {
        return false;
    }
    true
}

fn stable_attr_selector(node: &DomNodeSnapshot, tag: &str) -> Option<String> {
    for &hook in STABLE_HOOK_ATTRS {
        if let Some(attr) = node.attrs.iter().find(|a| {
            a.name.eq_ignore_ascii_case(hook)
                && a.unique_with_tag
                && !a.value.trim().is_empty()
                && a.value.len() <= 120
                && !a.value.contains('\n')
                && !a.value.contains('\r')
        }) {
            return Some(format!(
                "{tag}[{hook}=\"{}\"]",
                css_escape_attr_value(attr.value.trim())
            ));
        }
    }
    None
}

fn steady_classes(node: &DomNodeSnapshot) -> Vec<&str> {
    let mut out = Vec::new();
    for cls in &node.classes {
        if is_steady_class(cls) && !out.contains(&cls.as_str()) {
            out.push(cls.as_str());
        }
    }
    out
}

/// Genera un selector CSS estable, corto y determinista en Rust puro a partir de `ElementPickSnapshot`.
///
/// Prioridad:
/// 1. `#id` estable y único en el elemento objetivo.
/// 2. Atributo estable único (`data-testid`, `data-test`, `data-qa`, `data-cy`, `aria-label`, `name`, `role`).
/// 3. Clase estable única (o combinación corta de clases estables) en el objetivo.
/// 4. `#id` único del objetivo aunque parezca autogenerado (antes de recurrir a una ruta larga).
/// 5. Ruta mínima anclada en el ancestro más cercano con `#id` / atributo estable / clase única,
///    usando `:nth-of-type(n)` únicamente cuando existen hermanos con la misma etiqueta.
pub fn generate_selector(snapshot: &ElementPickSnapshot) -> Option<String> {
    let target = snapshot.ancestry.first()?;
    let target_tag = sanitize_tag(&target.tag)?;

    // 1. #id estable y único.
    if target.id_unique {
        if let Some(ref id) = target.id {
            let trimmed = id.trim();
            if is_steady_id(trimmed) {
                return Some(format!("#{}", css_escape_ident(trimmed)));
            }
        }
    }

    // 2. Atributo semántico único.
    if let Some(attr_sel) = stable_attr_selector(target, &target_tag) {
        return Some(attr_sel);
    }

    // 3. Clases estables únicas.
    let steady = steady_classes(target);
    if let Some(unique_single) = steady
        .iter()
        .find(|&&cls| target.unique_classes.iter().any(|u| u == cls))
    {
        return Some(format!("{target_tag}.{}", css_escape_ident(unique_single)));
    }
    if steady.len() >= 2 && steady.len() == target.classes.len() && target.classes_combo_unique {
        let joined = steady
            .iter()
            .take(4)
            .map(|c| css_escape_ident(c))
            .collect::<Vec<_>>()
            .join(".");
        return Some(format!("{target_tag}.{joined}"));
    }

    // 4. #id único (aunque no pase heurística de estabilidad) antes de caer en nth-of-type.
    if target.id_unique {
        if let Some(ref id) = target.id {
            let trimmed = id.trim();
            if !trimmed.is_empty() {
                return Some(format!("#{}", css_escape_ident(trimmed)));
            }
        }
    }

    // 5. Ruta mínima anclada hacia arriba evitando `nth-of-type` siempre que sea posible.
    let mut parts: Vec<String> = Vec::new();
    for (depth, node) in snapshot.ancestry.iter().enumerate() {
        let Some(tag) = sanitize_tag(&node.tag) else {
            break;
        };
        if tag == "html" {
            break;
        }
        if depth > 0 {
            if node.id_unique {
                if let Some(ref id) = node.id {
                    let trimmed = id.trim();
                    if is_steady_id(trimmed) {
                        parts.push(format!("#{}", css_escape_ident(trimmed)));
                        break;
                    }
                }
            }
            if let Some(attr_sel) = stable_attr_selector(node, &tag) {
                parts.push(attr_sel);
                break;
            }
            let anc_steady = steady_classes(node);
            if let Some(unique_single) = anc_steady
                .iter()
                .find(|&&cls| node.unique_classes.iter().any(|u| u == cls))
            {
                parts.push(format!("{tag}.{}", css_escape_ident(unique_single)));
                break;
            }
            if node.id_unique {
                if let Some(ref id) = node.id {
                    let trimmed = id.trim();
                    if !trimmed.is_empty() {
                        parts.push(format!("#{}", css_escape_ident(trimmed)));
                        break;
                    }
                }
            }
            if tag == "body" {
                parts.push("body".to_string());
                break;
            }
        }

        let node_steady = steady_classes(node);
        let segment = if node.same_tag_siblings > 1 {
            let idx = node.nth_of_type.max(1);
            if let Some(first_cls) = node_steady.first() {
                format!("{tag}.{}:nth-of-type({idx})", css_escape_ident(first_cls))
            } else {
                format!("{tag}:nth-of-type({idx})")
            }
        } else if let Some(first_cls) = node_steady.first() {
            format!("{tag}.{}", css_escape_ident(first_cls))
        } else {
            tag
        };
        parts.push(segment);
    }

    if parts.is_empty() {
        return None;
    }
    parts.reverse();
    Some(parts.join(" > "))
}

fn sanitize_tag(raw: &str) -> Option<String> {
    let lower = raw.trim().to_ascii_lowercase();
    if lower.is_empty()
        || !lower
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return None;
    }
    Some(lower)
}

/// Script inyectable para aplicar la hoja CSS de Curtain (`<style id="office-veil">`) sin parpadeo.
pub fn build_veil_style_script(css: &str) -> String {
    let literal = js_string_literal(css);
    format!(
        r#"(function(){{
  var css = {literal};
  function apply() {{
    var root = document.head || document.documentElement;
    if (!root) return false;
    var sheet = document.getElementById('office-veil');
    if (!sheet) {{
      if (!css) return true;
      sheet = document.createElement('style');
      sheet.id = 'office-veil';
      root.appendChild(sheet);
    }}
    sheet.textContent = css;
    return true;
  }}
  if (!apply()) {{
    document.addEventListener('DOMContentLoaded', apply, {{ once: true }});
  }}
}})();"#
    )
}

/// Script para previsualizar temporalmente un elemento oculto al pasar el cursor en el panel.
pub fn build_peek_script(css_without_selector: &str, selector: &str) -> String {
    let css_lit = js_string_literal(css_without_selector);
    let sel_lit = js_string_literal(selector);
    format!(
        r#"(function(){{
  var root = document.head || document.documentElement;
  if (!root) return;
  function sheet(id) {{
    var s = document.getElementById(id);
    if (!s) {{ s = document.createElement('style'); s.id = id; root.appendChild(s); }}
    return s;
  }}
  var sel = {sel_lit};
  sheet('office-veil').textContent = {css_lit};
  sheet('office-peek').textContent = sel + ' {{ outline: 2px solid rgba(23,23,23,.9) !important; outline-offset: 2px !important; }}';
  try {{
    var el = document.querySelector(sel);
    if (el) el.scrollIntoView({{ block: 'center', behavior: 'smooth' }});
  }} catch (e) {{}}
}})();"#
    )
}

/// Script para restaurar la hoja CSS de Curtain al dejar de previsualizar una fila del panel.
pub fn build_unpeek_script(full_css: &str) -> String {
    let css_lit = js_string_literal(full_css);
    format!(
        r#"(function(){{
  var veil = document.getElementById('office-veil');
  if (veil) veil.textContent = {css_lit};
  var peek = document.getElementById('office-peek');
  if (peek) peek.textContent = '';
}})();"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::SearchPaths;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_SEQ: AtomicU64 = AtomicU64::new(1);

    fn temp_curtain_path(label: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let n = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "search-curtain-test-{label}-{}-{n}",
            std::process::id()
        ));
        let sp = SearchPaths::with_base(&root);
        sp.ensure_dirs().expect("ensure_dirs");
        (root, sp.curtain_file())
    }

    fn node(tag: &str) -> DomNodeSnapshot {
        DomNodeSnapshot {
            tag: tag.to_string(),
            id: None,
            id_unique: false,
            classes: Vec::new(),
            unique_classes: Vec::new(),
            classes_combo_unique: false,
            attrs: Vec::new(),
            nth_of_type: 1,
            same_tag_siblings: 1,
        }
    }

    #[test]
    fn prefers_steady_unique_id_first() {
        let mut target = node("div");
        target.id = Some("gateway-content".to_string());
        target.id_unique = true;
        target.classes = vec!["paywall-modal".to_string()];
        target.unique_classes = vec!["paywall-modal".to_string()];

        let snap = ElementPickSnapshot {
            ancestry: vec![target, node("body")],
            label: "Dialog".to_string(),
            note: Some("600×420".to_string()),
        };
        assert_eq!(
            generate_selector(&snap).as_deref(),
            Some("#gateway-content")
        );
    }

    #[test]
    fn skips_generated_id_in_favor_of_data_testid() {
        let mut target = node("section");
        target.id = Some(":r12:".to_string());
        target.id_unique = true;
        target.attrs.push(DomAttrSnapshot {
            name: "data-testid".to_string(),
            value: "cookie-consent-banner".to_string(),
            unique_with_tag: true,
        });

        let snap = ElementPickSnapshot {
            ancestry: vec![target, node("body")],
            label: "Cookie banner".to_string(),
            note: None,
        };
        assert_eq!(
            generate_selector(&snap).as_deref(),
            Some(r#"section[data-testid="cookie-consent-banner"]"#)
        );
    }

    #[test]
    fn filters_hashed_css_classes_and_picks_steady_class() {
        assert!(!is_steady_class("css-1a2b3c"));
        assert!(!is_steady_class("sc-bdVaJa"));
        assert!(!is_steady_class("BasicByline_byline__VTyoS"));
        assert!(!is_steady_class("col-1234"));
        assert!(!is_steady_class("ab"));
        assert!(is_steady_class("newsletter-popup"));
        assert!(is_steady_class("cookie-banner"));

        let mut target = node("aside");
        target.classes = vec![
            "css-1x9abc".to_string(),
            "PromoBox_root__Xy7zQ".to_string(),
            "newsletter-popup".to_string(),
        ];
        target.unique_classes = vec!["newsletter-popup".to_string()];

        let snap = ElementPickSnapshot {
            ancestry: vec![target, node("main"), node("body")],
            label: "Sidebar".to_string(),
            note: None,
        };
        assert_eq!(
            generate_selector(&snap).as_deref(),
            Some("aside.newsletter-popup")
        );
    }

    #[test]
    fn builds_short_anchored_path_and_only_uses_nth_of_type_when_needed() {
        let mut target = node("div");
        target.classes = vec!["css-998877".to_string()];
        target.nth_of_type = 2;
        target.same_tag_siblings = 3;

        let mut wrapper = node("section");
        wrapper.classes = vec!["promo-rail".to_string()];
        wrapper.nth_of_type = 1;
        wrapper.same_tag_siblings = 1;

        let mut anchor = node("main");
        anchor.id = Some("article-main".to_string());
        anchor.id_unique = true;

        let snap = ElementPickSnapshot {
            ancestry: vec![target, wrapper, anchor, node("body")],
            label: "Promo".to_string(),
            note: None,
        };
        assert_eq!(
            generate_selector(&snap).as_deref(),
            Some("#article-main > section.promo-rail > div:nth-of-type(2)")
        );
    }

    #[test]
    fn css_escape_handles_special_chars_and_leading_digits() {
        assert_eq!(css_escape_ident("ad:slot.1"), r"ad\:slot\.1");
        assert_eq!(css_escape_ident("123banner"), r"\31 23banner");
        assert_eq!(
            css_escape_attr_value(r#"Subscribe "now" \ today"#),
            r#"Subscribe \"now\" \\ today"#
        );
    }

    #[test]
    fn curtain_store_persists_atomically_recovers_corrupt_bak_and_generates_isolated_css() {
        let (root, path) = temp_curtain_path("persist");
        let mut store = CurtainStore::default();
        assert!(store.hide(
            "https://www.nytimes.com/news",
            "#gateway-content".to_string(),
            "Dialog".to_string(),
            Some("600×420 · middle centre".to_string()),
            1795032000.0
        ));
        // Duplicado ignorado.
        assert!(!store.hide(
            "nytimes.com",
            "#gateway-content".to_string(),
            "Dialog".to_string(),
            None,
            1795032005.0
        ));
        assert!(store.hide(
            "nytimes.com",
            "aside.newsletter-popup".to_string(),
            "Newsletter".to_string(),
            None,
            1795032010.0
        ));

        let css = store.css_for_host("www.nytimes.com", None);
        assert_eq!(
            css,
            "#gateway-content { display: none !important; }\naside.newsletter-popup { display: none !important; }"
        );
        let peek_css = store.css_for_host("nytimes.com", Some("#gateway-content"));
        assert_eq!(
            peek_css,
            "aside.newsletter-popup { display: none !important; }"
        );

        store.save(&path).expect("save curtain.json");
        let reloaded = CurtainStore::load(&path);
        assert_eq!(reloaded, store);

        // Undo y restore.
        let mut mut_store = reloaded;
        let undone = mut_store.undo("nytimes.com").expect("undo");
        assert_eq!(undone.selector, "aside.newsletter-popup");
        assert_eq!(mut_store.veils("nytimes.com").len(), 1);
        assert_eq!(mut_store.restore_all("nytimes.com"), 1);
        assert!(mut_store.veils("nytimes.com").is_empty());

        // Corrupción -> respaldo .bak + arranque limpio.
        paths::write_atomic(&path, b"{corrupt curtain").expect("write corrupt");
        let recovered = CurtainStore::load(&path);
        assert_eq!(recovered.total_rules(), 0);
        assert!(path.with_extension("json.bak").exists());

        let _ = fs::remove_dir_all(&root);
    }

    /// Árbol DOM mínimo solo para tests que convierte HTML real de prueba en `ElementPickSnapshot`.
    #[derive(Debug, Clone)]
    struct HtmlNode {
        tag: String,
        id: Option<String>,
        classes: Vec<String>,
        attrs: Vec<(String, String)>,
        parent: Option<usize>,
        children: Vec<usize>,
    }

    fn parse_fixture_html(html: &str) -> Vec<HtmlNode> {
        let mut nodes: Vec<HtmlNode> = Vec::new();
        let mut stack: Vec<usize> = Vec::new();
        let bytes = html.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] != b'<' {
                i += 1;
                continue;
            }
            let Some(end) = html[i + 1..].find('>').map(|rel| i + 1 + rel) else {
                break;
            };
            let inside = html[i + 1..end].trim();
            i = end + 1;
            if inside.is_empty() || inside.starts_with('!') {
                continue;
            }
            if inside.starts_with('/') {
                stack.pop();
                continue;
            }
            let self_closing = inside.ends_with('/');
            let body = inside.trim_end_matches('/').trim();
            let mut parts = body.split_whitespace();
            let Some(tag_raw) = parts.next() else {
                continue;
            };
            let tag = tag_raw.to_ascii_lowercase();
            let rest = body[tag_raw.len()..].trim();
            let mut id = None;
            let mut classes = Vec::new();
            let mut attrs = Vec::new();
            let mut rest_idx = 0;
            let rbytes = rest.as_bytes();
            while rest_idx < rbytes.len() {
                while rest_idx < rbytes.len() && rbytes[rest_idx].is_ascii_whitespace() {
                    rest_idx += 1;
                }
                let k_start = rest_idx;
                while rest_idx < rbytes.len()
                    && !rbytes[rest_idx].is_ascii_whitespace()
                    && rbytes[rest_idx] != b'='
                {
                    rest_idx += 1;
                }
                let key = rest[k_start..rest_idx].to_ascii_lowercase();
                if key.is_empty() {
                    break;
                }
                let mut val = String::new();
                if rest_idx < rbytes.len() && rbytes[rest_idx] == b'=' {
                    rest_idx += 1;
                    if rest_idx < rbytes.len()
                        && (rbytes[rest_idx] == b'"' || rbytes[rest_idx] == b'\'')
                    {
                        let q = rbytes[rest_idx];
                        rest_idx += 1;
                        let v_start = rest_idx;
                        while rest_idx < rbytes.len() && rbytes[rest_idx] != q {
                            rest_idx += 1;
                        }
                        val = rest[v_start..rest_idx].to_string();
                        if rest_idx < rbytes.len() {
                            rest_idx += 1;
                        }
                    }
                }
                match key.as_str() {
                    "id" => id = Some(val),
                    "class" => {
                        classes = val.split_whitespace().map(str::to_string).collect();
                    }
                    _ => attrs.push((key, val)),
                }
            }

            let parent = stack.last().copied();
            let idx = nodes.len();
            nodes.push(HtmlNode {
                tag,
                id,
                classes,
                attrs,
                parent,
                children: Vec::new(),
            });
            if let Some(p) = parent {
                nodes[p].children.push(idx);
            }
            if !self_closing {
                stack.push(idx);
            }
        }
        nodes
    }

    fn snapshot_from_fixture(nodes: &[HtmlNode], target_idx: usize) -> ElementPickSnapshot {
        let mut ancestry = Vec::new();
        let mut cur = Some(target_idx);
        while let Some(idx) = cur {
            let n = &nodes[idx];
            let id_unique =
                n.id.as_ref()
                    .map(|id| nodes.iter().filter(|o| o.id.as_ref() == Some(id)).count() == 1)
                    .unwrap_or(false);
            let unique_classes = n
                .classes
                .iter()
                .filter(|c| {
                    nodes
                        .iter()
                        .filter(|o| o.tag == n.tag && o.classes.contains(c))
                        .count()
                        == 1
                })
                .cloned()
                .collect();
            let classes_combo_unique = !n.classes.is_empty()
                && nodes
                    .iter()
                    .filter(|o| o.tag == n.tag && n.classes.iter().all(|c| o.classes.contains(c)))
                    .count()
                    == 1;
            let attrs = STABLE_HOOK_ATTRS
                .iter()
                .filter_map(|&hook| {
                    let (_, val) = n.attrs.iter().find(|(k, _)| k == hook)?;
                    let unique = nodes
                        .iter()
                        .filter(|o| {
                            o.tag == n.tag && o.attrs.iter().any(|(ok, ov)| ok == hook && ov == val)
                        })
                        .count()
                        == 1;
                    Some(DomAttrSnapshot {
                        name: hook.to_string(),
                        value: val.clone(),
                        unique_with_tag: unique,
                    })
                })
                .collect();
            let (nth_of_type, same_tag_siblings) = match n.parent {
                Some(p) => {
                    let kin: Vec<usize> = nodes[p]
                        .children
                        .iter()
                        .copied()
                        .filter(|&c| nodes[c].tag == n.tag)
                        .collect();
                    let pos = kin
                        .iter()
                        .position(|&c| c == idx)
                        .map(|x| x + 1)
                        .unwrap_or(1);
                    (pos, kin.len().max(1))
                }
                None => (1, 1),
            };
            ancestry.push(DomNodeSnapshot {
                tag: n.tag.clone(),
                id: n.id.clone(),
                id_unique,
                classes: n.classes.clone(),
                unique_classes,
                classes_combo_unique,
                attrs,
                nth_of_type,
                same_tag_siblings,
            });
            cur = n.parent;
        }
        ElementPickSnapshot {
            ancestry,
            label: "Elemento".to_string(),
            note: None,
        }
    }

    #[test]
    fn generates_stable_selectors_from_realistic_html_fixture() {
        let html = r#"
        <html><body>
          <div id="cookie-consent" class="css-9a8b7c banner">Cookies</div>
          <main id="article-root">
            <section class="story-body">
              <div class="css-1a2b3c card">Uno</div>
              <div class="css-4d5e6f card">Dos</div>
            </section>
            <aside data-testid="newsletter-box" class="css-999111 rail">Suscríbete</aside>
          </main>
        </body></html>
        "#;
        let dom = parse_fixture_html(html);

        // 1. Banner con id estable -> `#cookie-consent`
        let cookie_idx = dom
            .iter()
            .position(|n| n.id.as_deref() == Some("cookie-consent"))
            .expect("cookie node");
        assert_eq!(
            generate_selector(&snapshot_from_fixture(&dom, cookie_idx)).as_deref(),
            Some("#cookie-consent")
        );

        // 2. Aside con data-testid único y clase hasheada -> `aside[data-testid="newsletter-box"]`
        let nl_idx = dom
            .iter()
            .position(|n| n.tag == "aside")
            .expect("newsletter node");
        assert_eq!(
            generate_selector(&snapshot_from_fixture(&dom, nl_idx)).as_deref(),
            Some(r#"aside[data-testid="newsletter-box"]"#)
        );

        // 3. Segundo div repetido con clases hasheadas -> anclado en `section.story-body`
        let second_card = dom
            .iter()
            .enumerate()
            .filter(|(_, n)| n.classes.iter().any(|c| c == "card"))
            .nth(1)
            .map(|(i, _)| i)
            .expect("second card");
        assert_eq!(
            generate_selector(&snapshot_from_fixture(&dom, second_card)).as_deref(),
            Some("section.story-body > div.card:nth-of-type(2)")
        );
    }

    #[test]
    fn toggle_rule_enabled_omits_from_css_without_deleting() {
        let mut store = CurtainStore::default();
        assert!(store.hide(
            "https://example.com",
            "#banner".into(),
            "Banner".into(),
            None,
            1.0
        ));
        assert!(store.css_for_host("example.com", None).contains("#banner"));

        assert!(store.set_enabled("example.com", "#banner", false));
        assert!(!store.set_enabled("example.com", "#banner", false));
        assert_eq!(store.veils("example.com").len(), 1);
        assert!(!store.veils("example.com")[0].enabled);
        assert!(store.css_for_host("example.com", None).is_empty());

        assert!(store.set_enabled("example.com", "#banner", true));
        assert!(store.css_for_host("example.com", None).contains("#banner"));
    }

    #[test]
    fn export_and_import_json_merges_without_duplicates() {
        let mut src = CurtainStore::default();
        src.hide(
            "https://example.com",
            "#cookie-bar".into(),
            "Cookies".into(),
            None,
            10.0,
        );
        src.hide(
            "https://news.example.org",
            ".promo".into(),
            "Promo".into(),
            Some("abajo".into()),
            20.0,
        );
        let json = src.export_json().expect("export");

        let mut dst = CurtainStore::default();
        dst.hide(
            "https://example.com",
            "#cookie-bar".into(),
            "Cookies".into(),
            None,
            5.0,
        );
        let stats = dst.import_json(&json).expect("import");
        assert_eq!(stats.added, 1);
        assert_eq!(stats.skipped, 1);
        assert_eq!(dst.total_rules(), 2);
    }
}
