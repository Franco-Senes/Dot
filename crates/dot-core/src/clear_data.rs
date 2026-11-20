//! Planificador de borrado de datos de navegación (`Clear Browsing Data`).
//!
//! Calcula el timestamp de corte según el rango temporal y genera un resumen
//! legible de los datos seleccionados para borrar.

use serde::{Deserialize, Serialize};

/// Rango temporal para el borrado selectivo de datos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClearRange {
    LastHour,
    #[default]
    Last24Hours,
    Last7Days,
    AllTime,
}

impl ClearRange {
    /// Etiqueta legible del rango temporal.
    pub fn label(self) -> &'static str {
        match self {
            Self::LastHour => "Última hora",
            Self::Last24Hours => "Últimas 24 horas",
            Self::Last7Days => "Últimos 7 días",
            Self::AllTime => "Desde siempre",
        }
    }

    /// Calcula el timestamp UNIX (en segundos) a partir del cual se borran datos.
    /// Devuelve `None` para [`ClearRange::AllTime`].
    pub fn cutoff_epoch(self, now_epoch: u64) -> Option<u64> {
        match self {
            Self::LastHour => Some(now_epoch.saturating_sub(3_600)),
            Self::Last24Hours => Some(now_epoch.saturating_sub(86_400)),
            Self::Last7Days => Some(now_epoch.saturating_sub(7 * 86_400)),
            Self::AllTime => None,
        }
    }
}

/// Banderas selectivas de categorías de datos a borrar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ClearDataFlags {
    pub history: bool,
    pub downloads: bool,
    pub closed_tabs: bool,
    pub site_prefs: bool,
    pub cookies_cache: bool,
}

impl ClearDataFlags {
    /// Todas las categorías desactivadas.
    pub fn none() -> Self {
        Self::default()
    }

    /// Todas las categorías activadas.
    pub fn all() -> Self {
        Self {
            history: true,
            downloads: true,
            closed_tabs: true,
            site_prefs: true,
            cookies_cache: true,
        }
    }

    /// Indica si no hay ninguna categoría seleccionada.
    pub fn is_empty(&self) -> bool {
        !self.history
            && !self.downloads
            && !self.closed_tabs
            && !self.site_prefs
            && !self.cookies_cache
    }

    /// Lista las etiquetas de categorías activas.
    pub fn selected_labels(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.history {
            out.push("Historial");
        }
        if self.downloads {
            out.push("Descargas");
        }
        if self.closed_tabs {
            out.push("Pestañas cerradas");
        }
        if self.site_prefs {
            out.push("Preferencias por sitio");
        }
        if self.cookies_cache {
            out.push("Cookies y caché");
        }
        out
    }
}

/// Plan concreto de borrado con rango, banderas y timestamp de corte calculado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClearDataPlan {
    pub range: ClearRange,
    pub flags: ClearDataFlags,
    pub cutoff_epoch: Option<u64>,
}

/// Resumen estructurado del plan de borrado listo para confirmación en UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClearDataSummary {
    pub range_label: String,
    pub targets: Vec<String>,
    pub cutoff_epoch: Option<u64>,
    pub is_empty: bool,
}

impl ClearDataPlan {
    /// Construye un plan de borrado para `range`, `flags` y el instante `now_epoch`.
    pub fn new(range: ClearRange, flags: ClearDataFlags, now_epoch: u64) -> Self {
        Self {
            range,
            flags,
            cutoff_epoch: range.cutoff_epoch(now_epoch),
        }
    }

    /// Indica si una entrada con marca temporal `item_epoch` entra dentro del rango a borrar.
    pub fn matches_timestamp(&self, item_epoch: u64) -> bool {
        match self.cutoff_epoch {
            Some(cutoff) => item_epoch >= cutoff,
            None => true,
        }
    }

    /// Genera un resumen legible del plan.
    pub fn summarize(&self) -> ClearDataSummary {
        let targets: Vec<String> = self
            .flags
            .selected_labels()
            .into_iter()
            .map(str::to_string)
            .collect();
        ClearDataSummary {
            range_label: self.range.label().to_string(),
            is_empty: targets.is_empty(),
            targets,
            cutoff_epoch: self.cutoff_epoch,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cutoff_calculations_are_accurate() {
        let now = 1_700_000_000u64;
        assert_eq!(ClearRange::LastHour.cutoff_epoch(now), Some(now - 3_600));
        assert_eq!(
            ClearRange::Last24Hours.cutoff_epoch(now),
            Some(now - 86_400)
        );
        assert_eq!(
            ClearRange::Last7Days.cutoff_epoch(now),
            Some(now - 7 * 86_400)
        );
        assert_eq!(ClearRange::AllTime.cutoff_epoch(now), None);
        assert_eq!(ClearRange::LastHour.cutoff_epoch(100), Some(0));
    }

    #[test]
    fn plan_matches_timestamp_and_summarizes() {
        let now = 100_000u64;
        let flags = ClearDataFlags {
            history: true,
            closed_tabs: true,
            cookies_cache: true,
            ..ClearDataFlags::none()
        };
        let plan = ClearDataPlan::new(ClearRange::LastHour, flags, now);
        assert!(plan.matches_timestamp(now - 100));
        assert!(!plan.matches_timestamp(now - 7_200));

        let summary = plan.summarize();
        assert!(!summary.is_empty);
        assert_eq!(summary.range_label, "Última hora");
        assert_eq!(
            summary.targets,
            vec!["Historial", "Pestañas cerradas", "Cookies y caché"]
        );
    }

    #[test]
    fn empty_flags_produce_empty_summary() {
        let plan = ClearDataPlan::new(ClearRange::AllTime, ClearDataFlags::none(), 50_000);
        assert!(plan.matches_timestamp(0));
        let summary = plan.summarize();
        assert!(summary.is_empty);
        assert!(summary.targets.is_empty());
    }
}
