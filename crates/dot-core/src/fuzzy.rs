//! Coincidencia difusa rápida en Rust puro para el selector de pestañas (`Ctrl+K`)
//! y el autocompletado de Fase 3 (`Fuzzy.swift`).
//!
//! Combina tres niveles ordenados de prioridad por término de búsqueda:
//! 1. Subcadena exacta (`memchr::memmem`) con bonificación por prefijo e inicio de palabra.
//! 2. Subsecuencia ordenada (abreviatura estilo `"dwn"` → `"downloads"`) con bonificación
//!    por caracteres consecutivos e inicios de palabra.
//! 3. Respaldo tolerante a erratas mediante distancia Damerau-Levenshtein (`strsim::osa_distance`)
//!    para términos de ≥ 4 letras (1 errata para 4–7 letras, 2 erratas para ≥ 8 letras),
//!    siempre puntuado por debajo de las coincidencias exactas.

use memchr::memmem;
use strsim::osa_distance;

/// Evalúa qué tan bien coincide `query` sobre un texto `candidate`.
/// Devuelve `Some(score)` (mayor es mejor) si todos los términos coinciden, o `None` si no.
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<i64> {
    let q_lower = query.trim().to_lowercase();
    if q_lower.is_empty() {
        return Some(0);
    }
    let text_lower = candidate.to_lowercase();
    if text_lower.is_empty() {
        return None;
    }

    let mut total: i64 = 0;
    let mut term_count: i64 = 0;
    for term in q_lower.split_whitespace() {
        let term_score = score_single_term(term, &text_lower)?;
        total += term_score;
        term_count += 1;
    }

    if term_count == 0 {
        return Some(0);
    }

    // Pequeña bonificación si la consulta completa aparece contigua en el texto.
    if term_count > 1 && memmem::find(text_lower.as_bytes(), q_lower.as_bytes()).is_some() {
        total += 45;
    }

    // Preferir candidatos más concisos a igualdad de coincidencia.
    let len_penalty = (text_lower.chars().count().min(160) as i64) / 4;
    Some(total - len_penalty)
}

/// Puntúa una pestaña abierta (título + URL) frente a `query` para `Ctrl+K`.
/// Las coincidencias sobre el título pesan más que las de la URL, y una consulta vacía
/// coincide con todas las pestañas conservando su orden natural.
pub fn fuzzy_match_tab(query: &str, title: &str, url: &str) -> Option<i64> {
    let q_lower = query.trim().to_lowercase();
    if q_lower.is_empty() {
        return Some(0);
    }

    let title_lower = title.to_lowercase();
    let url_lower = url.to_lowercase();

    let mut total: i64 = 0;
    let mut all_in_title = true;

    for term in q_lower.split_whitespace() {
        let t_score = if title_lower.is_empty() {
            None
        } else {
            score_single_term(term, &title_lower)
        };
        let u_score = if url_lower.is_empty() {
            None
        } else {
            score_single_term(term, &url_lower)
        };

        match (t_score, u_score) {
            (Some(ts), Some(us)) => {
                total += ts * 3 + us / 2 + 20;
            }
            (Some(ts), None) => {
                total += ts * 3;
            }
            (None, Some(us)) => {
                all_in_title = false;
                total += us;
            }
            (None, None) => return None,
        }
    }

    if all_in_title && !title_lower.is_empty() {
        total += 35;
    }

    let len_penalty = (title_lower.chars().count().min(120) as i64) / 4;
    Some(total - len_penalty)
}

/// Puntúa un único término ya en minúsculas contra `text` ya en minúsculas.
fn score_single_term(term: &str, text: &str) -> Option<i64> {
    if term.is_empty() {
        return Some(0);
    }

    let term_bytes = term.as_bytes();
    let text_bytes = text.as_bytes();

    // 1. Subcadena exacta (prioridad máxima).
    if let Some(best_exact) = best_substring_score(term_bytes, text_bytes) {
        return Some(best_exact);
    }

    // 2. Subsecuencia ordenada / abreviatura (prioridad media).
    if let Some(subseq_score) = subsequence_score(term, text) {
        return Some(subseq_score);
    }

    // 3. Respaldo Damerau-Levenshtein para erratas en términos de ≥ 4 caracteres.
    typo_fallback_score(term, text)
}

fn best_substring_score(term: &[u8], text: &[u8]) -> Option<i64> {
    let finder = memmem::Finder::new(term);
    let mut offset = 0usize;
    let mut best: Option<i64> = None;

    while offset <= text.len() {
        let Some(rel) = finder.find(&text[offset..]) else {
            break;
        };
        let pos = offset + rel;
        let mut score: i64 = 200;

        if pos == 0 {
            score += 90;
        } else if !is_word_byte(text[pos - 1]) {
            score += 65;
        }

        let end = pos + term.len();
        if end == text.len() || !is_word_byte(text[end]) {
            score += 20;
        }

        // Cuanto antes aparezca en la cadena, ligeramente mejor.
        score -= (pos.min(64) as i64) / 2;

        best = Some(match best {
            Some(prev) => prev.max(score),
            None => score,
        });

        // Ya tenemos la mejor puntuación posible en el prefijo exacto.
        if pos == 0 {
            break;
        }
        offset = pos + 1;
    }

    best
}

fn subsequence_score(term: &str, text: &str) -> Option<i64> {
    let q_chars: Vec<char> = term.chars().collect();
    if q_chars.is_empty() {
        return Some(0);
    }

    let t_chars: Vec<char> = text.chars().collect();
    if q_chars.len() > t_chars.len() {
        return None;
    }

    let mut qi = 0usize;
    let mut score: i64 = 80;
    let mut prev_match: Option<usize> = None;
    let mut first_match: Option<usize> = None;

    for (ti, &tc) in t_chars.iter().enumerate() {
        if tc == q_chars[qi] {
            if first_match.is_none() {
                first_match = Some(ti);
            }
            if ti == 0 || !is_word_char(t_chars[ti - 1]) {
                score += 18;
            }
            if let Some(prev) = prev_match {
                if ti == prev + 1 {
                    score += 14;
                } else {
                    let gap = (ti - prev - 1).min(12) as i64;
                    score -= gap * 2;
                }
            }
            prev_match = Some(ti);
            qi += 1;
            if qi == q_chars.len() {
                break;
            }
        }
    }

    if qi != q_chars.len() {
        return None;
    }

    // Evitar falsos positivos dispersos en abreviaturas muy cortas si no empieza en frontera de palabra.
    if q_chars.len() >= 2 {
        if let Some(start) = first_match {
            let span = prev_match.unwrap_or(start) - start + 1;
            if span > q_chars.len() * 6 {
                return None;
            }
        }
    }

    Some(score.max(30))
}

fn typo_fallback_score(term: &str, text: &str) -> Option<i64> {
    let q_len = term.chars().count();
    let allowed = if q_len < 4 {
        0
    } else if q_len < 8 {
        1
    } else {
        2
    };
    if allowed == 0 {
        return None;
    }

    let q_chars: Vec<char> = term.chars().collect();
    let mut best_dist: Option<usize> = None;

    for word in text.split(|c: char| !is_word_char(c)) {
        if word.is_empty() {
            continue;
        }
        let w_chars: Vec<char> = word.chars().collect();
        if w_chars.len() < 2 || w_chars.len() + allowed < q_len {
            continue;
        }

        // Igual que `Fuzzy.swift`: exige coincidencia en la primera letra o transposición con la segunda.
        let first_ok = w_chars[0] == q_chars[0]
            || (q_chars.len() >= 2 && w_chars[0] == q_chars[1] && w_chars[1] == q_chars[0]);
        if !first_ok {
            continue;
        }

        // Comparar contra la palabra completa o contra su prefijo de longitud comparable.
        let mut word_best = osa_distance(term, word);
        let prefix_len = w_chars.len().min(q_len + allowed);
        if prefix_len < w_chars.len() {
            let prefix: String = w_chars[..prefix_len].iter().collect();
            word_best = word_best.min(osa_distance(term, &prefix));
        }
        if w_chars.len() >= q_len {
            let exact_prefix: String = w_chars[..q_len].iter().collect();
            word_best = word_best.min(osa_distance(term, &exact_prefix));
        }

        if word_best <= allowed {
            best_dist = Some(match best_dist {
                Some(prev) => prev.min(word_best),
                None => word_best,
            });
            if word_best == 1 {
                break;
            }
        }
    }

    best_dist.map(|d| 25 - (d as i64) * 8)
}

#[inline]
fn is_word_byte(b: u8) -> bool {
    b >= 0x80 || b.is_ascii_alphanumeric()
}

#[inline]
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_matches_everything() {
        assert_eq!(fuzzy_score("", "Rust Programming Language"), Some(0));
        assert_eq!(
            fuzzy_match_tab("   ", "Example", "https://example.com/"),
            Some(0)
        );
    }

    #[test]
    fn prefix_and_word_boundary_rank_above_midword_substring() {
        let prefix = fuzzy_score("api", "API Reference").expect("prefix match");
        let boundary = fuzzy_score("api", "team/api docs").expect("boundary match");
        let midword = fuzzy_score("api", "rapid development").expect("midword match");

        assert!(prefix > boundary, "prefix={prefix} boundary={boundary}");
        assert!(boundary > midword, "boundary={boundary} midword={midword}");
    }

    #[test]
    fn multi_term_and_subsequence_abbreviation_work() {
        assert!(fuzzy_score("rust docs", "The Rust Standard Library Docs").is_some());
        assert!(fuzzy_score("dwn", "Downloads").is_some());
        assert!(fuzzy_score("ghrs", "GitHub Rust").is_some());
        assert!(fuzzy_score("rust python", "The Rust Book").is_none());
    }

    #[test]
    fn damerau_levenshtein_typo_fallback_for_four_plus_letters() {
        // Transposición ("gihub" -> "github") y letra errónea ("githib" -> "github").
        let typo_swap = fuzzy_score("gihub", "GitHub — Build software").expect("swap typo");
        let exact = fuzzy_score("github", "GitHub — Build software").expect("exact match");
        assert!(exact > typo_swap, "exact={exact} typo={typo_swap}");

        assert!(fuzzy_score("setings", "Browser Settings").is_some());
        // Menos de 4 letras no dispara heurística de erratas para evitar ruido.
        assert!(fuzzy_score("xyz", "Example Domain").is_none());
    }

    #[test]
    fn tab_matching_ranks_title_above_url_only() {
        let title_hit =
            fuzzy_match_tab("search", "Search Linux", "https://example.com/").expect("title hit");
        let url_only = fuzzy_match_tab("search", "Inicio", "https://duckduckgo.com/?q=search")
            .expect("url hit");
        assert!(title_hit > url_only, "title={title_hit} url={url_only}");
    }
}
