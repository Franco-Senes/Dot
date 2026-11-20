//! Limpiador de parámetros de rastreo en URLs e inspector de componentes URL.
//!
//! Elimina parámetros conocidos (`utm_*`, `fbclid`, `gclid`, `yclid`, `mc_cid`,
//! `mc_eid`, `igshid`, `si`, `ref_src`, `_hsenc`, etc.) sin alterar el resto de la consulta.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use url::Url;

const TRACKING_PARAMS: &[&str] = &[
    "fbclid",
    "gclid",
    "gbraid",
    "wbraid",
    "dclid",
    "yclid",
    "msclkid",
    "mc_cid",
    "mc_eid",
    "igshid",
    "igsh",
    "si",
    "ref_src",
    "ref_url",
    "_hsenc",
    "_hsmi",
    "hsctatracking",
    "mkt_tok",
    "vero_id",
    "oly_enc_id",
    "oly_anon_id",
    "s_kwcid",
    "wickedid",
    "epik",
    "ttclid",
];

/// Indica si el nombre de un parámetro de consulta es un rastreador conocido.
pub fn is_tracking_param(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("utm_") || TRACKING_PARAMS.contains(&lower.as_str())
}

/// Resultado de limpiar una URL de parámetros de rastreo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CleanUrlResult {
    pub cleaned_url: String,
    pub removed_params: Vec<String>,
    pub removed_count: usize,
}

/// Limpia los parámetros de rastreo de `raw_url`, devolviendo la URL limpia y el conteo eliminado.
pub fn clean_url(raw_url: &str) -> Result<CleanUrlResult> {
    let mut parsed =
        Url::parse(raw_url.trim()).with_context(|| format!("URL inválida: {raw_url}"))?;

    let mut kept: Vec<(String, String)> = Vec::new();
    let mut removed_params: Vec<String> = Vec::new();

    for (k, v) in parsed.query_pairs() {
        if is_tracking_param(&k) {
            removed_params.push(k.into_owned());
        } else {
            kept.push((k.into_owned(), v.into_owned()));
        }
    }

    let removed_count = removed_params.len();
    if removed_count > 0 {
        if kept.is_empty() {
            parsed.set_query(None);
        } else {
            let mut pairs = parsed.query_pairs_mut();
            pairs.clear();
            for (k, v) in &kept {
                pairs.append_pair(k, v);
            }
        }
    }

    Ok(CleanUrlResult {
        cleaned_url: parsed.to_string(),
        removed_params,
        removed_count,
    })
}

/// Par clave-valor decodificado de una consulta URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryParam {
    pub key: String,
    pub value: String,
    pub is_tracking: bool,
}

/// Desglose estructurado de una URL para el inspector de direcciones.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UrlInspection {
    pub scheme: String,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub path: String,
    pub query_params: Vec<QueryParam>,
    pub fragment: Option<String>,
    pub is_secure: bool,
}

/// Inspecciona una URL devolviendo esquema, host, puerto, ruta, parámetros y fragmento.
pub fn inspect_url(raw_url: &str) -> Result<UrlInspection> {
    let parsed = Url::parse(raw_url.trim()).with_context(|| format!("URL inválida: {raw_url}"))?;

    let query_params = parsed
        .query_pairs()
        .map(|(k, v)| {
            let key = k.into_owned();
            let is_tracking = is_tracking_param(&key);
            QueryParam {
                key,
                value: v.into_owned(),
                is_tracking,
            }
        })
        .collect();

    Ok(UrlInspection {
        scheme: parsed.scheme().to_string(),
        host: parsed.host_str().map(str::to_string),
        port: parsed.port(),
        path: parsed.path().to_string(),
        query_params,
        fragment: parsed.fragment().map(str::to_string),
        is_secure: parsed.scheme().eq_ignore_ascii_case("https"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_utm_and_click_ids_preserving_legitimate_params() {
        let res = clean_url(
            "https://example.com/article?id=42&utm_source=news&utm_medium=email&fbclid=abc&gclid=xyz&page=2#top",
        )
        .unwrap();
        assert_eq!(res.removed_count, 4);
        assert_eq!(
            res.removed_params,
            vec!["utm_source", "utm_medium", "fbclid", "gclid"]
        );
        assert_eq!(
            res.cleaned_url,
            "https://example.com/article?id=42&page=2#top"
        );
    }

    #[test]
    fn removes_query_mark_when_all_params_stripped() {
        let res = clean_url("https://youtu.be/dQw4w9WgXcQ?si=track123&igshid=abc").unwrap();
        assert_eq!(res.removed_count, 2);
        assert_eq!(res.cleaned_url, "https://youtu.be/dQw4w9WgXcQ");
    }

    #[test]
    fn clean_url_noop_when_no_tracking_params() {
        let res = clean_url("https://docs.rs/serde/latest/serde/?search=Serialize").unwrap();
        assert_eq!(res.removed_count, 0);
        assert!(res.removed_params.is_empty());
        assert_eq!(
            res.cleaned_url,
            "https://docs.rs/serde/latest/serde/?search=Serialize"
        );
    }

    #[test]
    fn inspect_url_extracts_all_components() {
        let info =
            inspect_url("https://example.org:8443/v1/search?q=rust&utm_campaign=fall#res").unwrap();
        assert_eq!(info.scheme, "https");
        assert_eq!(info.host.as_deref(), Some("example.org"));
        assert_eq!(info.port, Some(8443));
        assert_eq!(info.path, "/v1/search");
        assert_eq!(info.fragment.as_deref(), Some("res"));
        assert!(info.is_secure);
        assert_eq!(info.query_params.len(), 2);
        assert!(!info.query_params[0].is_tracking);
        assert!(info.query_params[1].is_tracking);
    }

    #[test]
    fn rejects_invalid_url() {
        assert!(clean_url("not a url").is_err());
        assert!(inspect_url("://bad").is_err());
    }
}
