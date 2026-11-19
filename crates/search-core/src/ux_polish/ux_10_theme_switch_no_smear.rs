//! UX-10: Supresión de transiciones al cambiar de tema para evitar barridos visuales.
//!
//! Basado en `better-ui` ("Suppress transitions on theme switch") y `better-writing`:
//! al cambiar entre tema claro, oscuro o del sistema, todas las transiciones CSS
//! deben desactivarse temporalmente, forzar un volcado síncrono de estilos
//! (`style flush`) y restaurarse tras el siguiente frame pintado.

/// Especificación declarativa de la regla UX-10.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub summary: &'static str,
    pub css_override: &'static str,
    pub scoped_css_override: &'static str,
    pub scoped_class: &'static str,
    pub media_query: &'static str,
    pub style_flush_required: bool,
    pub restore_after_frames: u8,
    pub action_label_es: &'static str,
    pub switch_to_dark_label_es: &'static str,
    pub switch_to_light_label_es: &'static str,
    pub use_system_theme_label_es: &'static str,
}

/// Regla CSS global exacta para suprimir transiciones durante el cambio de tema.
pub const THEME_SWITCH_DISABLE_TRANSITIONS_CSS: &str =
    "*,*::before,*::after{transition:none !important}";

/// Clase CSS auxiliar para contenedores (por ejemplo GTK4 o WebView) durante el cambio.
pub const THEME_SWITCHING_CLASS: &str = "theme-switching";

/// Regla CSS con alcance por clase para suprimir transiciones sin inyectar nodos `<style>`.
pub const THEME_SWITCH_SCOPED_CSS: &str =
    ".theme-switching,.theme-switching *,.theme-switching *::before,.theme-switching *::after{transition:none !important}";

/// Media query del sistema operativo observada para cambios automáticos de tema.
pub const PREFERS_COLOR_SCHEME_DARK_QUERY: &str = "(prefers-color-scheme: dark)";

/// Número de `requestAnimationFrame` anidados antes de retirar la supresión.
pub const RESTORE_AFTER_ANIMATION_FRAMES: u8 = 2;

/// Etiquetas en español (sentence case, verbo primero, sin signos de exclamación).
pub const ACTION_LABEL_ES: &str = "Cambiar tema";
pub const SWITCH_TO_DARK_LABEL_ES: &str = "Cambiar a tema oscuro";
pub const SWITCH_TO_LIGHT_LABEL_ES: &str = "Cambiar a tema claro";
pub const USE_SYSTEM_THEME_LABEL_ES: &str = "Usar tema del sistema";

/// Devuelve la especificación completa de la regla UX-10.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_10_theme_switch_no_smear",
        title: "Suprimir transiciones al cambiar de tema",
        category: "motion_and_surfaces",
        summary: "Desactiva las transiciones CSS, fuerza un volcado síncrono de estilos y restaura las transiciones tras dos frames de animación para evitar que los colores se barran al unísono.",
        css_override: THEME_SWITCH_DISABLE_TRANSITIONS_CSS,
        scoped_css_override: THEME_SWITCH_SCOPED_CSS,
        scoped_class: THEME_SWITCHING_CLASS,
        media_query: PREFERS_COLOR_SCHEME_DARK_QUERY,
        style_flush_required: true,
        restore_after_frames: RESTORE_AFTER_ANIMATION_FRAMES,
        action_label_es: ACTION_LABEL_ES,
        switch_to_dark_label_es: SWITCH_TO_DARK_LABEL_ES,
        switch_to_light_label_es: SWITCH_TO_LIGHT_LABEL_ES,
        use_system_theme_label_es: USE_SYSTEM_THEME_LABEL_ES,
    }
}

/// Modo de tema activo en la interfaz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
    System,
}

impl ThemeMode {
    /// Devuelve la etiqueta de acción en español para cambiar hacia este modo.
    pub const fn action_label_es(self) -> &'static str {
        match self {
            Self::Light => SWITCH_TO_LIGHT_LABEL_ES,
            Self::Dark => SWITCH_TO_DARK_LABEL_ES,
            Self::System => USE_SYSTEM_THEME_LABEL_ES,
        }
    }

    /// Devuelve el anuncio accesible tras completar el cambio de tema.
    pub const fn announcement_es(self) -> &'static str {
        match self {
            Self::Light => "Tema cambiado a claro",
            Self::Dark => "Tema cambiado a oscuro",
            Self::System => "Tema sincronizado con el sistema",
        }
    }
}

/// Fase del ciclo de supresión de transiciones durante un cambio de tema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeSwitchPhase {
    /// Sin cambio en curso; las transiciones normales están activas.
    Idle,
    /// Hoja o clase de supresión aplicada antes de confirmar los nuevos colores.
    TransitionsSuppressed,
    /// Volcado síncrono de estilos ejecutado con la supresión activa.
    StyleFlushed,
    /// Primer `requestAnimationFrame` completado; pendiente el frame de pintado.
    WaitingPaintFrame,
}

/// Controlador puro de estado para coordinar el cambio de tema sin barrido visual.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeSwitchController {
    current_theme: ThemeMode,
    phase: ThemeSwitchPhase,
    pending_frames: u8,
    flush_count: u32,
}

impl ThemeSwitchController {
    /// Crea un controlador en reposo con el tema inicial indicado.
    pub const fn new(initial_theme: ThemeMode) -> Self {
        Self {
            current_theme: initial_theme,
            phase: ThemeSwitchPhase::Idle,
            pending_frames: 0,
            flush_count: 0,
        }
    }

    /// Devuelve el tema actualmente aplicado.
    pub const fn current_theme(&self) -> ThemeMode {
        self.current_theme
    }

    /// Devuelve la fase actual del ciclo de supresión.
    pub const fn phase(&self) -> ThemeSwitchPhase {
        self.phase
    }

    /// Indica si las transiciones CSS deben permanecer suprimidas en este instante.
    pub const fn is_transition_suppressed(&self) -> bool {
        !matches!(self.phase, ThemeSwitchPhase::Idle)
    }

    /// Devuelve cuántos volcados síncronos de estilo se han ejecutado.
    pub const fn flush_count(&self) -> u32 {
        self.flush_count
    }

    /// Inicia un cambio de tema activando la supresión de transiciones.
    ///
    /// Si el tema solicitado ya está activo y el controlador está en reposo,
    /// no inicia supresión innecesaria y devuelve `false`.
    pub fn begin_switch(&mut self, target: ThemeMode) -> bool {
        if self.current_theme == target && self.phase == ThemeSwitchPhase::Idle {
            return false;
        }
        self.current_theme = target;
        self.phase = ThemeSwitchPhase::TransitionsSuppressed;
        self.pending_frames = RESTORE_AFTER_ANIMATION_FRAMES;
        true
    }

    /// Registra el volcado síncrono de estilos (`offsetHeight` o equivalente GTK).
    ///
    /// Devuelve `true` si la supresión estaba activa y avanzó a `StyleFlushed`.
    pub fn flush_style(&mut self) -> bool {
        if self.phase != ThemeSwitchPhase::TransitionsSuppressed {
            return false;
        }
        self.phase = ThemeSwitchPhase::StyleFlushed;
        self.flush_count = self.flush_count.saturating_add(1);
        true
    }

    /// Avanza un frame de animación (`requestAnimationFrame`).
    ///
    /// Solo restaura las transiciones (`Idle`) después del volcado síncrono y de
    /// completar los `RESTORE_AFTER_ANIMATION_FRAMES` frames requeridos.
    pub fn on_animation_frame(&mut self) -> ThemeSwitchPhase {
        match self.phase {
            ThemeSwitchPhase::Idle | ThemeSwitchPhase::TransitionsSuppressed => {
                // No se restauran transiciones antes de ejecutar el volcado síncrono.
                self.phase
            }
            ThemeSwitchPhase::StyleFlushed => {
                self.pending_frames = self.pending_frames.saturating_sub(1);
                if self.pending_frames == 0 {
                    self.phase = ThemeSwitchPhase::Idle;
                } else {
                    self.phase = ThemeSwitchPhase::WaitingPaintFrame;
                }
                self.phase
            }
            ThemeSwitchPhase::WaitingPaintFrame => {
                self.pending_frames = self.pending_frames.saturating_sub(1);
                if self.pending_frames == 0 {
                    self.phase = ThemeSwitchPhase::Idle;
                }
                self.phase
            }
        }
    }
}

/// Genera el fragmento JavaScript estándar para suprimir transiciones en WebView/DOM
/// al detectar cambios en `(prefers-color-scheme: dark)` o al alternar el tema.
pub fn dom_suppression_script() -> &'static str {
    concat!(
        "(() => {",
        "const s=document.createElement('style');",
        "s.textContent='*,*::before,*::after{transition:none !important}';",
        "document.head.append(s);",
        "void document.body.offsetHeight;",
        "requestAnimationFrame(()=>requestAnimationFrame(()=>s.remove()));",
        "})();"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_matches_exact_suppression_values() {
        let rule = spec();
        assert_eq!(rule.id, "ux_10_theme_switch_no_smear");
        assert_eq!(
            rule.css_override,
            "*,*::before,*::after{transition:none !important}"
        );
        assert_eq!(rule.media_query, "(prefers-color-scheme: dark)");
        assert!(rule.style_flush_required);
        assert_eq!(rule.restore_after_frames, 2);
        assert_eq!(rule.action_label_es, "Cambiar tema");
        assert_eq!(rule.switch_to_dark_label_es, "Cambiar a tema oscuro");
        assert_eq!(rule.switch_to_light_label_es, "Cambiar a tema claro");
        assert_eq!(rule.use_system_theme_label_es, "Usar tema del sistema");
    }

    #[test]
    fn controller_suppresses_flushes_and_restores_after_two_frames() {
        let mut ctrl = ThemeSwitchController::new(ThemeMode::Light);
        assert!(!ctrl.is_transition_suppressed());
        assert!(!ctrl.begin_switch(ThemeMode::Light));

        assert!(ctrl.begin_switch(ThemeMode::Dark));
        assert_eq!(ctrl.current_theme(), ThemeMode::Dark);
        assert!(ctrl.is_transition_suppressed());
        assert_eq!(ctrl.phase(), ThemeSwitchPhase::TransitionsSuppressed);

        // No debe restaurar antes del volcado síncrono de estilos.
        assert_eq!(
            ctrl.on_animation_frame(),
            ThemeSwitchPhase::TransitionsSuppressed
        );
        assert!(ctrl.is_transition_suppressed());

        assert!(ctrl.flush_style());
        assert_eq!(ctrl.flush_count(), 1);
        assert_eq!(ctrl.phase(), ThemeSwitchPhase::StyleFlushed);
        assert!(ctrl.is_transition_suppressed());

        // Primer frame: sigue suprimido hasta el frame de pintado.
        assert_eq!(
            ctrl.on_animation_frame(),
            ThemeSwitchPhase::WaitingPaintFrame
        );
        assert!(ctrl.is_transition_suppressed());

        // Segundo frame: restaura las transiciones.
        assert_eq!(ctrl.on_animation_frame(), ThemeSwitchPhase::Idle);
        assert!(!ctrl.is_transition_suppressed());
    }

    #[test]
    fn dom_script_and_spanish_copy_are_valid() {
        let script = dom_suppression_script();
        assert!(script.contains(THEME_SWITCH_DISABLE_TRANSITIONS_CSS));
        assert!(script.contains("offsetHeight"));
        assert!(script.contains("requestAnimationFrame"));

        assert_eq!(ThemeMode::Dark.action_label_es(), "Cambiar a tema oscuro");
        assert_eq!(ThemeMode::Dark.announcement_es(), "Tema cambiado a oscuro");
        assert!(!ThemeMode::System.announcement_es().contains('!'));
    }
}
