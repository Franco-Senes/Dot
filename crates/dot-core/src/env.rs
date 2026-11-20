//! Resolución de variables de entorno `DOT_*` con respaldo retrocompatible para los nombres
//! heredados `SEARCH_*` (emitiendo una advertencia de obsolescencia).

use std::ffi::OsString;

/// Resultado de consultar una variable `DOT_*` con respaldo opcional `SEARCH_*`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvLookup {
    pub value: String,
    pub used_legacy: bool,
}

/// Lee `primary` (`DOT_*`); si no está definida o está vacía, lee `legacy` (`SEARCH_*`)
/// registrando una advertencia de obsolescencia mediante `dot_core::warn!`.
pub fn var(primary: &str, legacy: &str) -> Option<String> {
    let found = lookup_with(primary, legacy, |k| std::env::var(k).ok())?;
    if found.used_legacy {
        crate::warn!(
            "La variable de entorno '{legacy}' está obsoleta; usa '{primary}' en su lugar"
        );
    }
    Some(found.value)
}

/// Igual que [`var`], pero conservando `OsString` (útil para rutas de sistema).
pub fn var_os(primary: &str, legacy: &str) -> Option<OsString> {
    if let Some(val) = std::env::var_os(primary).filter(|v| !v.is_empty()) {
        return Some(val);
    }
    let val = std::env::var_os(legacy).filter(|v| !v.is_empty())?;
    crate::warn!("La variable de entorno '{legacy}' está obsoleta; usa '{primary}' en su lugar");
    Some(val)
}

/// Devuelve `true` si la variable (`primary` o respaldo `legacy`) vale `1`, `true` o `yes`.
pub fn flag_enabled(primary: &str, legacy: &str) -> bool {
    var(primary, legacy)
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// Devuelve `true` si la variable (`primary` o respaldo `legacy`) está definida y no es `0`/`false`.
pub fn truthy(primary: &str, legacy: &str) -> bool {
    var(primary, legacy).is_some_and(|v| {
        let t = v.trim();
        !t.is_empty() && t != "0" && !t.eq_ignore_ascii_case("false")
    })
}

/// Lógica pura de resolución con respaldo (inyectable para tests deterministas sin mutar el entorno global).
pub fn lookup_with<F>(primary: &str, legacy: &str, mut get: F) -> Option<EnvLookup>
where
    F: FnMut(&str) -> Option<String>,
{
    if let Some(val) = get(primary) {
        if !val.trim().is_empty() {
            return Some(EnvLookup {
                value: val,
                used_legacy: false,
            });
        }
    }
    if let Some(val) = get(legacy) {
        if !val.trim().is_empty() {
            return Some(EnvLookup {
                value: val,
                used_legacy: true,
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn primary_dot_var_takes_precedence_over_legacy_search_var() {
        let map = HashMap::from([
            ("DOT_LOG", "debug".to_string()),
            ("SEARCH_LOG", "error".to_string()),
        ]);
        let res =
            lookup_with("DOT_LOG", "SEARCH_LOG", |k| map.get(k).cloned()).expect("should resolve");
        assert_eq!(res.value, "debug");
        assert!(!res.used_legacy);
    }

    #[test]
    fn legacy_search_var_is_used_as_fallback_and_marked_deprecated() {
        let map = HashMap::from([("SEARCH_CHROMIUM_FLAGS", "--process-per-site".to_string())]);
        let res = lookup_with("DOT_CHROMIUM_FLAGS", "SEARCH_CHROMIUM_FLAGS", |k| {
            map.get(k).cloned()
        })
        .expect("should fall back to legacy");
        assert_eq!(res.value, "--process-per-site");
        assert!(res.used_legacy);
    }

    #[test]
    fn blank_primary_falls_back_to_legacy_and_missing_returns_none() {
        let map = HashMap::from([
            ("DOT_BENCH", "   ".to_string()),
            ("SEARCH_BENCH", "1".to_string()),
        ]);
        let res = lookup_with("DOT_BENCH", "SEARCH_BENCH", |k| map.get(k).cloned())
            .expect("blank primary should fall back");
        assert_eq!(res.value, "1");
        assert!(res.used_legacy);

        let empty: HashMap<&str, String> = HashMap::new();
        assert!(lookup_with("DOT_SOCKET", "SEARCH_SOCKET", |k| empty.get(k).cloned()).is_none());
    }
}
