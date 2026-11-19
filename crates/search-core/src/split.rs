//! Modelo de dominio para Split View (`Split.swift`).
//!
//! Agrupa entre 2 y 4 pestañas lado a lado (o apiladas según `SplitAxis`),
//! gestionando proporciones válidas, foco activo, intercambio, separación y
//! conversión bidireccional con `SessionSplit` para `session.json`.

use crate::session::{SessionSplit, SplitAxis};

/// Número máximo de paneles simultáneos en un split (igual que `TabSplit.most` en `Split.swift`).
pub const MAX_SPLIT_PANES: usize = 4;

/// Presets de proporción para Split View de 2 paneles (`50/50`, `65/35`, `35/65`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitRatioPreset {
    Even,
    LeftWide,
    RightWide,
}

impl SplitRatioPreset {
    pub const ALL: [Self; 3] = [Self::Even, Self::LeftWide, Self::RightWide];

    /// Fracción del panel izquierdo asociada al preset.
    pub const fn fraction(self) -> f64 {
        match self {
            Self::Even => 0.50,
            Self::LeftWide => 0.65,
            Self::RightWide => 0.35,
        }
    }

    /// Etiqueta compacta para la interfaz (`50/50`, `65/35`, `35/65`).
    pub const fn label(self) -> &'static str {
        match self {
            Self::Even => "50/50",
            Self::LeftWide => "65/35",
            Self::RightWide => "35/65",
        }
    }

    /// Devuelve el preset más cercano si `fraction` está dentro de `tolerance`.
    pub fn from_fraction(fraction: f64, tolerance: f64) -> Option<Self> {
        if !fraction.is_finite() {
            return None;
        }
        Self::ALL
            .into_iter()
            .find(|preset| (preset.fraction() - fraction).abs() <= tolerance)
    }
}

/// Representa un grupo de pestañas mostradas simultáneamente en Split View.
#[derive(Debug, Clone, PartialEq)]
pub struct TabSplit {
    pub id: u64,
    /// IDs de pestañas en orden visual (izquierda -> derecha en horizontal).
    pub tabs: Vec<u64>,
    pub axis: SplitAxis,
    /// Fracción de cada panel respecto al ancho/alto total (suman 1.0).
    pub sizes: Vec<f64>,
    /// Última pestaña enfocada dentro de este split.
    pub focused: Option<u64>,
    /// Estado de sincronización de desplazamiento entre paneles.
    pub sync_scroll: bool,
}

impl TabSplit {
    /// Crea un split de dos paneles lado a lado con proporción inicial `fraction` (p. ej. `0.5`).
    pub fn new_pair(id: u64, left: u64, right: u64, fraction: f64) -> Self {
        let sizes = Self::clamp_sizes(&[fraction, 1.0 - fraction]);
        Self {
            id,
            tabs: vec![left, right],
            axis: SplitAxis::Horizontal,
            sizes,
            focused: Some(right),
            sync_scroll: false,
        }
    }

    /// Crea un split a partir de una lista de pestañas (mínimo 2) y proporciones opcionales.
    pub fn new(
        id: u64,
        tabs: Vec<u64>,
        axis: SplitAxis,
        sizes: Option<Vec<f64>>,
        focused: Option<u64>,
    ) -> Option<Self> {
        if tabs.len() < 2 || tabs.len() > MAX_SPLIT_PANES {
            return None;
        }
        let clean_sizes = match sizes {
            Some(s) if s.len() == tabs.len() => Self::clamp_sizes(&s),
            _ => Self::even_sizes(tabs.len()),
        };
        let focused = focused
            .filter(|f| tabs.contains(f))
            .or_else(|| tabs.first().copied());
        Some(Self {
            id,
            tabs,
            axis,
            sizes: clean_sizes,
            focused,
            sync_scroll: false,
        })
    }

    /// Proporciones iguales para `count` paneles.
    pub fn even_sizes(count: usize) -> Vec<f64> {
        let n = count.max(1);
        vec![1.0 / (n as f64); n]
    }

    /// Fracción mínima permitida por panel: 0.2 para 2 paneles, 0.1 para 3-4 paneles.
    pub fn least_share(count: usize) -> f64 {
        if count <= 2 {
            0.2
        } else {
            0.1
        }
    }

    /// Normaliza y acota las proporciones para que ningún panel sea demasiado estrecho
    /// y la suma sea exactamente 1.0.
    pub fn clamp_sizes(sizes: &[f64]) -> Vec<f64> {
        if sizes.is_empty() {
            return Vec::new();
        }
        if sizes.len() == 2 {
            let first = if sizes[0].is_finite() {
                sizes[0].clamp(0.2, 0.8)
            } else {
                0.5
            };
            return vec![first, 1.0 - first];
        }
        let finite: Vec<f64> = sizes
            .iter()
            .map(|&s| if s.is_finite() && s > 0.0 { s } else { 1.0 })
            .collect();
        let total: f64 = finite.iter().sum();
        if !total.is_finite() || total <= 0.0 {
            return Self::even_sizes(sizes.len());
        }
        let mut shares: Vec<f64> = finite.into_iter().map(|s| s / total).collect();
        let least = Self::least_share(shares.len());
        let short: f64 = shares.iter().map(|&s| (least - s).max(0.0)).sum();
        if short <= 0.0 {
            return shares;
        }
        let spare: f64 = shares.iter().map(|&s| (s - least).max(0.0)).sum();
        if spare <= 0.0 {
            return Self::even_sizes(shares.len());
        }
        for share in &mut shares {
            if *share < least {
                *share = least;
            } else {
                *share -= (*share - least) / spare * short;
            }
        }
        shares
    }

    /// Primer panel (izquierda).
    pub fn left(&self) -> Option<u64> {
        self.tabs.first().copied()
    }

    /// Último panel (derecha).
    pub fn right(&self) -> Option<u64> {
        self.tabs.last().copied()
    }

    /// Proporción del primer panel (útil para splits de 2 paneles).
    pub fn primary_fraction(&self) -> f64 {
        self.sizes.first().copied().unwrap_or(0.5).clamp(0.2, 0.8)
    }

    /// Actualiza la proporción del primer panel en un split de 2 paneles.
    pub fn set_primary_fraction(&mut self, fraction: f64) {
        if self.tabs.len() == 2 {
            self.sizes = Self::clamp_sizes(&[fraction, 1.0 - fraction]);
        }
    }

    /// Aplica uno de los presets (`50/50`, `65/35`, `35/65`) en un split de 2 paneles.
    pub fn apply_ratio_preset(&mut self, preset: SplitRatioPreset) {
        self.set_primary_fraction(preset.fraction());
    }

    /// Devuelve el preset activo si la proporción actual coincide dentro de `0.025`.
    pub fn active_ratio_preset(&self) -> Option<SplitRatioPreset> {
        if self.tabs.len() != 2 {
            return None;
        }
        SplitRatioPreset::from_fraction(self.primary_fraction(), 0.025)
    }

    /// Restablece todos los paneles a tamaños iguales (50/50 para 2 paneles).
    pub fn reset_even(&mut self) {
        self.sizes = Self::even_sizes(self.tabs.len());
    }

    /// Activa o desactiva la sincronización de desplazamiento entre paneles.
    pub fn set_sync_scroll(&mut self, enabled: bool) {
        self.sync_scroll = enabled;
    }

    /// Alterna el estado de sincronización de desplazamiento y devuelve el nuevo valor.
    pub fn toggle_sync_scroll(&mut self) -> bool {
        self.sync_scroll = !self.sync_scroll;
        self.sync_scroll
    }

    pub fn is_full(&self) -> bool {
        self.tabs.len() >= MAX_SPLIT_PANES
    }

    pub fn contains(&self, tab_id: u64) -> bool {
        self.tabs.contains(&tab_id)
    }

    /// Devuelve el panel compañero de `tab_id` (el siguiente a la derecha, o el de la izquierda si es el último).
    pub fn partner(&self, tab_id: u64) -> Option<u64> {
        let idx = self.tabs.iter().position(|&id| id == tab_id)?;
        if self.tabs.len() < 2 {
            return None;
        }
        if idx + 1 < self.tabs.len() {
            Some(self.tabs[idx + 1])
        } else {
            Some(self.tabs[idx - 1])
        }
    }

    /// Todos los paneles excepto `tab_id`, manteniendo el orden.
    pub fn others(&self, tab_id: u64) -> Vec<u64> {
        self.tabs
            .iter()
            .copied()
            .filter(|&id| id != tab_id)
            .collect()
    }

    /// Intercambia el orden de los paneles (p. ej. izquierda <-> derecha).
    pub fn swap(&mut self) {
        self.tabs.reverse();
        self.sizes.reverse();
    }

    /// Alias semántico de `swap` para intercambiar el panel izquierdo y derecho.
    pub fn swap_panes(&mut self) {
        self.swap();
    }

    /// Inserta una pestaña junto a `neighbour` (después por defecto, o antes si `before = true`).
    pub fn insert(&mut self, tab_id: u64, neighbour: Option<u64>, before: bool) -> bool {
        if self.is_full() || self.contains(tab_id) {
            return false;
        }
        let pos = neighbour
            .and_then(|n| self.tabs.iter().position(|&id| id == n))
            .map(|idx| if before { idx } else { idx + 1 })
            .unwrap_or(self.tabs.len());
        self.tabs.insert(pos, tab_id);
        self.sizes = Self::even_sizes(self.tabs.len());
        true
    }

    /// Elimina `tab_id` del split. Devuelve `true` si aún quedan al menos 2 paneles,
    /// o `false` si el split debe disolverse.
    pub fn remove(&mut self, tab_id: u64) -> bool {
        let Some(idx) = self.tabs.iter().position(|&id| id == tab_id) else {
            return self.tabs.len() >= 2;
        };
        self.tabs.remove(idx);
        if idx < self.sizes.len() {
            self.sizes.remove(idx);
        }
        if self.sizes.len() != self.tabs.len() {
            self.sizes = Self::even_sizes(self.tabs.len());
        } else if !self.tabs.is_empty() {
            self.sizes = Self::clamp_sizes(&self.sizes);
        }
        if self.focused == Some(tab_id) {
            self.focused = self.tabs.first().copied();
        }
        self.tabs.len() >= 2
    }

    /// Convierte este split en `SessionSplit` mapeando IDs de pestaña a índices de `SessionWindow.tabs`.
    pub fn to_session_split(
        &self,
        index_of: impl Fn(u64) -> Option<usize>,
    ) -> Option<SessionSplit> {
        let mut indices = Vec::with_capacity(self.tabs.len());
        let mut sizes = Vec::with_capacity(self.tabs.len());
        for (i, &tab_id) in self.tabs.iter().enumerate() {
            if let Some(tab_idx) = index_of(tab_id) {
                if !indices.contains(&tab_idx) {
                    indices.push(tab_idx);
                    sizes.push(self.sizes.get(i).copied().unwrap_or(0.5));
                }
            }
        }
        if indices.len() < 2 {
            return None;
        }
        let focused = self.focused.and_then(index_of);
        Some(SessionSplit {
            tabs: indices,
            axis: self.axis,
            sizes: Self::clamp_sizes(&sizes),
            focused,
        })
    }

    /// Reconstruye un `TabSplit` desde `SessionSplit` resolviendo índices a IDs reales de pestaña.
    pub fn from_session_split(
        id: u64,
        saved: &SessionSplit,
        tab_at: impl Fn(usize) -> Option<u64>,
    ) -> Option<Self> {
        let mut tabs = Vec::with_capacity(saved.tabs.len());
        let mut sizes = Vec::with_capacity(saved.tabs.len());
        for (i, &idx) in saved.tabs.iter().enumerate() {
            if tabs.len() >= MAX_SPLIT_PANES {
                break;
            }
            if let Some(tab_id) = tab_at(idx) {
                if !tabs.contains(&tab_id) {
                    tabs.push(tab_id);
                    sizes.push(saved.sizes.get(i).copied().unwrap_or(0.5));
                }
            }
        }
        let focused = saved.focused.and_then(tab_at);
        Self::new(id, tabs, saved.axis, Some(sizes), focused)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_sizes_enforces_bounds_and_sum_to_one() {
        let two = TabSplit::clamp_sizes(&[0.05, 0.95]);
        assert!((two[0] - 0.2).abs() < 1e-6);
        assert!((two[1] - 0.8).abs() < 1e-6);

        let nan_case = TabSplit::clamp_sizes(&[f64::NAN, 0.3]);
        assert!((nan_case[0] - 0.5).abs() < 1e-6);
        assert!((nan_case[1] - 0.5).abs() < 1e-6);

        let three = TabSplit::clamp_sizes(&[0.01, 0.5, 0.49]);
        let sum: f64 = three.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6);
        assert!(three.iter().all(|&s| s >= 0.1 - 1e-6));
    }

    #[test]
    fn partner_swap_insert_and_remove_behave_like_upstream() {
        let mut split = TabSplit::new_pair(1, 10, 20, 0.35);
        assert_eq!(split.partner(10), Some(20));
        assert_eq!(split.partner(20), Some(10));
        assert_eq!(split.partner(99), None);

        split.swap();
        assert_eq!(split.tabs, vec![20, 10]);
        assert!((split.primary_fraction() - 0.65).abs() < 1e-6);

        assert!(split.insert(30, Some(20), false));
        assert_eq!(split.tabs, vec![20, 30, 10]);
        assert_eq!(split.sizes.len(), 3);

        // Removing one of three leaves two -> keeps split alive
        assert!(split.remove(30));
        assert_eq!(split.tabs, vec![20, 10]);

        // Removing one of two leaves one -> split dissolves
        assert!(!split.remove(20));
    }

    #[test]
    fn session_split_roundtrip_filters_invalid_indices() {
        let split = TabSplit::new_pair(7, 100, 200, 0.42);
        let session = split
            .to_session_split(|id| match id {
                100 => Some(1),
                200 => Some(3),
                _ => None,
            })
            .expect("valid session split");

        assert_eq!(session.tabs, vec![1, 3]);
        assert_eq!(session.focused, Some(3));
        assert!((session.sizes[0] - 0.42).abs() < 1e-6);

        let restored = TabSplit::from_session_split(7, &session, |idx| match idx {
            1 => Some(100),
            3 => Some(200),
            _ => None,
        })
        .expect("restored split");

        assert_eq!(restored.tabs, vec![100, 200]);
        assert_eq!(restored.focused, Some(200));
        assert!((restored.primary_fraction() - 0.42).abs() < 1e-6);

        // Invalid indices (< 2 valid tabs) returns None
        let invalid = SessionSplit {
            tabs: vec![0, 99],
            axis: SplitAxis::Horizontal,
            sizes: vec![0.5, 0.5],
            focused: None,
        };
        assert!(
            TabSplit::from_session_split(8, &invalid, |idx| (idx == 0).then_some(10)).is_none()
        );
    }

    #[test]
    fn ratio_presets_and_sync_scroll_behave_consistently() {
        let mut split = TabSplit::new_pair(9, 1, 2, 0.5);
        assert_eq!(split.active_ratio_preset(), Some(SplitRatioPreset::Even));
        assert!(!split.sync_scroll);

        split.apply_ratio_preset(SplitRatioPreset::LeftWide);
        assert!((split.primary_fraction() - 0.65).abs() < 1e-6);
        assert_eq!(
            split.active_ratio_preset(),
            Some(SplitRatioPreset::LeftWide)
        );
        assert_eq!(SplitRatioPreset::LeftWide.label(), "65/35");

        split.swap_panes();
        assert_eq!(split.tabs, vec![2, 1]);
        assert!((split.primary_fraction() - 0.35).abs() < 1e-6);
        assert_eq!(
            split.active_ratio_preset(),
            Some(SplitRatioPreset::RightWide)
        );
        assert_eq!(SplitRatioPreset::RightWide.label(), "35/65");

        assert!(split.toggle_sync_scroll());
        assert!(split.sync_scroll);
        split.set_sync_scroll(false);
        assert!(!split.sync_scroll);
    }
}
