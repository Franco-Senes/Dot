//! Inicialización del motor CEF 156 (`cef-rs`), despacho temprano de subprocesos,
//! aplicación de `chromium_flags.rs` (`CommandLine` + preferencias de `RequestContext`),
//! bombeo no bloqueante integrado en el bucle principal de GLib y apagado limpio.

use anyhow::{bail, Context, Result};
use cef::{rc::Rc as _, *};
use dot_core::chromium_flags::{
    ChromiumConfig, ContextPreference, PrefValue, ResolvedChromiumFlags,
};
use dot_core::paths::{self, DotPaths};
use dot_core::settings::AppSettings;
use gnome_platform::glib::{self, ControlFlow, SourceId};
use std::cell::Cell;
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static CEF_PUMP_ACTIVE: AtomicBool = AtomicBool::new(false);
static IDLE_PUMP_QUEUED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static IN_MESSAGE_LOOP_WORK: Cell<bool> = const { Cell::new(false) };
}

/// Ejecuta una iteración no bloqueante de `cef::do_message_loop_work()` protegida contra reentrada.
pub fn pump_cef_once() {
    if !CEF_PUMP_ACTIVE.load(Ordering::Acquire) {
        return;
    }
    IN_MESSAGE_LOOP_WORK.with(|busy| {
        if busy.get() {
            return;
        }
        busy.set(true);
        cef::do_message_loop_work();
        busy.set(false);
    });
}

fn schedule_pump_from_cef(delay_ms: i64) {
    if !CEF_PUMP_ACTIVE.load(Ordering::Acquire) {
        return;
    }
    if delay_ms <= 0 {
        if !IDLE_PUMP_QUEUED.swap(true, Ordering::AcqRel) {
            glib::idle_add_once(|| {
                IDLE_PUMP_QUEUED.store(false, Ordering::Release);
                pump_cef_once();
            });
        }
    } else {
        let clamped_ms = delay_ms.min(50) as u64;
        glib::timeout_add_once(Duration::from_millis(clamped_ms), || {
            pump_cef_once();
        });
    }
}

wrap_browser_process_handler! {
    struct DotBrowserProcessHandler;

    impl BrowserProcessHandler {
        fn on_schedule_message_pump_work(&self, delay_ms: i64) {
            schedule_pump_from_cef(delay_ms);
        }
    }
}

wrap_app! {
    struct DotCefApp {
        resolved_flags: ResolvedChromiumFlags,
    }

    impl App {
        fn on_before_command_line_processing(
            &self,
            process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            let is_browser_process = process_type
                .map(|s| s.to_string().is_empty())
                .unwrap_or(true);
            if !is_browser_process {
                return;
            }
            let Some(cmd) = command_line else {
                return;
            };

            for (name, value) in self.resolved_flags.to_command_line_pairs() {
                let cef_name = CefString::from(name.as_str());
                match value {
                    Some(val) => {
                        let cef_val = CefString::from(val.as_str());
                        cmd.append_switch_with_value(Some(&cef_name), Some(&cef_val));
                    }
                    None => {
                        cmd.append_switch(Some(&cef_name));
                    }
                }
            }
        }

        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(DotBrowserProcessHandler::new())
        }
    }
}

/// Crea la instancia `cef::App` usada por subprocesos en `cef::execute_process` antes de cargar GTK4.
pub fn create_early_cef_app() -> cef::App {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let cfg = ChromiumConfig {
        headless_software_gl: detect_headless_software_gl(),
        no_sandbox: detect_no_sandbox(exe_dir.as_deref()),
        ozone_platform: detect_ozone_platform(),
        ..ChromiumConfig::default()
    };
    let resolved = ResolvedChromiumFlags::from_env(&cfg);
    DotCefApp::new(resolved)
}

/// Detecta si el entorno actual exige `no_sandbox = 1` en `CefSettings`
/// (Flatpak/`bwrap` con `PR_GET_NO_NEW_PRIVS`, `DOT_NO_CEF_SANDBOX=1` o `chrome-sandbox`
/// local sin bit setuid-root `04755`).
pub fn detect_no_sandbox(exe_dir: Option<&Path>) -> bool {
    if Path::new("/.flatpak-info").exists()
        || dot_core::env::flag_enabled("DOT_NO_CEF_SANDBOX", "SEARCH_NO_CEF_SANDBOX")
    {
        return true;
    }

    // Dentro de bubblewrap (Flatpak o sandbox de desarrollo) `no_new_privs` vale 1,
    // lo que impide usar `chrome-sandbox` setuid y suele bloquear `CLONE_NEWUSER` anidado.
    let no_new_privs = unsafe { libc::prctl(libc::PR_GET_NO_NEW_PRIVS, 0, 0, 0, 0) };
    if no_new_privs == 1 {
        return true;
    }

    if let Some(dir) = exe_dir {
        let helper = dir.join("chrome-sandbox");
        if helper.exists() && !is_setuid_root(&helper) {
            return true;
        }
    }

    false
}

#[cfg(unix)]
fn is_setuid_root(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match std::fs::metadata(path) {
        Ok(meta) => meta.uid() == 0 && (meta.mode() & 0o4000) != 0,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn is_setuid_root(_path: &Path) -> bool {
    false
}

/// Detecta si corremos sobre renderizado por software explícito (p. ej. Xvfb con Mesa software).
pub fn detect_headless_software_gl() -> bool {
    dot_core::env::flag_enabled("DOT_HEADLESS_SW", "SEARCH_HEADLESS_SW")
        || env_flag_enabled("LIBGL_ALWAYS_SOFTWARE")
}

/// Sincroniza el backend `--ozone-platform` de Chromium con el servidor gráfico real que usa GTK4
/// (Wayland cuando el socket Wayland es accesible y no se forzó `GDK_BACKEND=x11`; X11 en sesiones
/// X11 o bajo `sdk-xvfb`; `headless` si no existe ninguno).
pub fn detect_ozone_platform() -> Option<String> {
    let gdk_backend = std::env::var("GDK_BACKEND")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if gdk_backend == "x11" {
        return Some("x11".to_string());
    }

    if let Ok(wl_disp) = std::env::var("WAYLAND_DISPLAY") {
        let wl_trimmed = wl_disp.trim();
        if !wl_trimmed.is_empty() {
            let wl_path = Path::new(wl_trimmed);
            let exists = if wl_path.is_absolute() {
                wl_path.exists()
            } else if let Ok(rt_dir) = std::env::var("XDG_RUNTIME_DIR") {
                !rt_dir.trim().is_empty() && Path::new(rt_dir.trim()).join(wl_trimmed).exists()
            } else {
                false
            };
            if exists {
                return Some("wayland".to_string());
            }
        }
    }

    if std::env::var("DISPLAY")
        .map(|d| !d.trim().is_empty())
        .unwrap_or(false)
    {
        return Some("x11".to_string());
    }

    Some("headless".to_string())
}

fn env_flag_enabled(name: &str) -> bool {
    std::env::var(name)
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// Motor CEF activo en el proceso principal del navegador.
pub struct CefEngine {
    persistent_context: RequestContext,
    resolved_flags: ResolvedChromiumFlags,
    heartbeat_source: Cell<Option<SourceId>>,
    shut_down: Cell<bool>,
}

impl CefEngine {
    /// Inicializa CEF en modo OSR (`windowless_rendering_enabled = 1`, `external_message_pump = 1`),
    /// aplica las preferencias de privacidad al `RequestContext` persistente y arranca el bombeo GLib.
    pub fn initialize(paths: &DotPaths, app_settings: &AppSettings) -> Result<Self> {
        let exe_path = std::env::current_exe()
            .context("No se pudo obtener la ruta del ejecutable actual para CEF")?;
        let exe_dir = exe_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let no_sandbox = detect_no_sandbox(Some(&exe_dir));
        let headless_sw = detect_headless_software_gl();
        let ozone_platform = detect_ozone_platform();

        let chromium_cfg = app_settings.to_chromium_config(headless_sw, no_sandbox, ozone_platform);
        let resolved_flags = ResolvedChromiumFlags::from_env(&chromium_cfg);
        debug_assert!(
            resolved_flags.is_site_isolation_intact(),
            "Site Isolation jamás debe desactivarse"
        );

        let root_cache_dir = paths.cef_cache_dir();
        let profile_cache_dir = root_cache_dir.join("profile");
        paths::create_dir_0700(&root_cache_dir)?;
        paths::create_dir_0700(&profile_cache_dir)?;

        let cef_res_dir = resolve_cef_resources_dir(&exe_dir);
        let locales_dir = cef_res_dir.join("locales");

        let log_severity = if dot_core::env::var("DOT_LOG", "SEARCH_LOG")
            .map(|v| v.eq_ignore_ascii_case("debug") || v.eq_ignore_ascii_case("trace"))
            .unwrap_or(false)
        {
            LogSeverity::WARNING
        } else {
            LogSeverity::ERROR
        };

        let cef_settings = Settings {
            no_sandbox: i32::from(no_sandbox),
            browser_subprocess_path: CefString::from(exe_path.to_string_lossy().as_ref()),
            windowless_rendering_enabled: 1,
            external_message_pump: 1,
            root_cache_path: CefString::from(root_cache_dir.to_string_lossy().as_ref()),
            cache_path: CefString::from(profile_cache_dir.to_string_lossy().as_ref()),
            persist_session_cookies: 1,
            log_severity,
            resources_dir_path: CefString::from(cef_res_dir.to_string_lossy().as_ref()),
            locales_dir_path: CefString::from(locales_dir.to_string_lossy().as_ref()),
            background_color: 0xff1c1c1e,
            ..Default::default()
        };

        CEF_PUMP_ACTIVE.store(true, Ordering::Release);

        // Pasamos a CEF únicamente `argv[0]` para que nuestras flags propias (`--smoke-test`)
        // no interfieran con los argumentos posicionales internos de Chromium.
        let argv0 = std::env::args()
            .next()
            .unwrap_or_else(|| exe_path.to_string_lossy().into_owned());
        let c_argv0 = CString::new(argv0.replace('\0', "")).unwrap_or_default();
        let mut argv_ptrs = [c_argv0.as_ptr() as *mut libc::c_char];
        let main_args = MainArgs {
            argc: 1,
            argv: argv_ptrs.as_mut_ptr(),
        };
        let mut cef_app = DotCefApp::new(resolved_flags.clone());

        let ok = cef::initialize(
            Some(&main_args),
            Some(&cef_settings),
            Some(&mut cef_app),
            std::ptr::null_mut(),
        );
        if ok != 1 {
            CEF_PUMP_ACTIVE.store(false, Ordering::Release);
            bail!("cef::initialize devolvió código {ok}");
        }

        // Respaldo periódico suave (10 ms) para garantizar fluidez constante junto con
        // `on_schedule_message_pump_work`.
        let heartbeat = glib::timeout_add_local(Duration::from_millis(10), || {
            if !CEF_PUMP_ACTIVE.load(Ordering::Acquire) {
                return ControlFlow::Break;
            }
            pump_cef_once();
            ControlFlow::Continue
        });

        // Bombeo inicial corto para que el contexto global esté listo antes de fijar preferencias.
        pump_cef_once();

        let persistent_context = cef::request_context_get_global_context()
            .context("No se pudo obtener el RequestContext global persistente de CEF")?;
        apply_context_preferences(&persistent_context, resolved_flags.preferences());

        dot_core::info!(
            "Motor CEF inicializado (no_sandbox={}, headless_sw={}, switches={}, disabled_features={})",
            no_sandbox,
            headless_sw,
            resolved_flags.switches().len(),
            resolved_flags.disabled_features().len()
        );

        Ok(Self {
            persistent_context,
            resolved_flags,
            heartbeat_source: Cell::new(Some(heartbeat)),
            shut_down: Cell::new(false),
        })
    }

    /// Devuelve una referencia clonada al `RequestContext` persistente compartido por las pestañas normales.
    pub fn persistent_context(&self) -> RequestContext {
        self.persistent_context.clone()
    }

    /// Crea un `RequestContext` efímero aislado en memoria (`cache_path = ""`, sin tocar disco),
    /// usado por las ventanas privadas con las mismas preferencias de privacidad aplicadas.
    pub fn create_ephemeral_context(&self) -> Option<RequestContext> {
        let settings = RequestContextSettings {
            cache_path: CefString::default(),
            persist_session_cookies: 0,
            ..Default::default()
        };
        let ctx = cef::request_context_create_context(Some(&settings), None)?;
        apply_context_preferences(&ctx, self.resolved_flags.preferences());
        Some(ctx)
    }

    #[allow(dead_code)]
    pub fn resolved_flags(&self) -> &ResolvedChromiumFlags {
        &self.resolved_flags
    }

    /// Detiene el bombeo de GLib y apaga CEF limpiamente una única vez.
    pub fn shutdown(&self) {
        if self.shut_down.replace(true) {
            return;
        }
        if let Some(src) = self.heartbeat_source.take() {
            src.remove();
        }
        pump_cef_once();
        CEF_PUMP_ACTIVE.store(false, Ordering::Release);
        cef::shutdown();
        dot_core::debug!("Motor CEF apagado limpiamente");
    }
}

impl Drop for CefEngine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn resolve_cef_resources_dir(exe_dir: &Path) -> PathBuf {
    if exe_dir.join("icudtl.dat").exists() && exe_dir.join("resources.pak").exists() {
        return exe_dir.to_path_buf();
    }
    if let Ok(cef_env) = std::env::var("CEF_PATH") {
        let p = PathBuf::from(cef_env);
        if p.join("icudtl.dat").exists() {
            return p;
        }
    }
    exe_dir.to_path_buf()
}

pub fn apply_context_preferences(ctx: &RequestContext, prefs: &[ContextPreference]) {
    for pref in prefs {
        let Some(val) = cef::value_create() else {
            continue;
        };
        match &pref.value {
            PrefValue::Bool(b) => {
                val.set_bool(i32::from(*b));
            }
            PrefValue::Int(i) => {
                val.set_int(*i);
            }
            PrefValue::String(s) => {
                let cs = CefString::from(s.as_str());
                val.set_string(Some(&cs));
            }
        }
        let name = CefString::from(pref.name);
        let mut err = CefString::default();
        let mut val_mut = val;
        let ok = ctx.set_preference(Some(&name), Some(&mut val_mut), Some(&mut err));
        if ok == 0 {
            let err_txt = err.to_string();
            if !err_txt.is_empty() {
                dot_core::debug!(
                    "No se pudo fijar preferencia '{}' en RequestContext: {}",
                    pref.name,
                    err_txt
                );
            }
        }
    }
}
