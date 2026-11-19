//! Almacén reactivo de ajustes en la capa GUI: combina la persistencia atómica JSON
//! (`$XDG_CONFIG_HOME/search/settings.json`) con sincronización opcional sobre `GSettings`
//! (`io.github.searchlinux.Search`) cuando el esquema compilado está disponible.

use gnome_platform::gio;
use search_core::address::SearchEngine;
use search_core::settings::{AppSettings, ColorLook};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub const GSETTINGS_SCHEMA_ID: &str = "io.github.searchlinux.Search";

type Listener = Rc<dyn Fn(&AppSettings)>;

#[derive(Clone)]
pub struct SettingsStore {
    inner: Rc<RefCell<SettingsStoreInner>>,
}

struct SettingsStoreInner {
    file_path: PathBuf,
    settings: AppSettings,
    gsettings: Option<gio::Settings>,
    next_listener_id: u64,
    listeners: Vec<(u64, Listener)>,
}

impl SettingsStore {
    /// Carga `settings.json` y, si existe el esquema `GSettings` (instalado o en `./data`),
    /// sincroniza los valores iniciales hacia `GSettings` sin fallar si no está instalado.
    pub fn load(file_path: PathBuf) -> Self {
        let local_schema_dir = find_local_schema_dir();
        let gsettings = gio::Settings::try_new(GSETTINGS_SCHEMA_ID, local_schema_dir.as_deref());

        let file_existed = file_path.is_file();
        let mut settings = AppSettings::load(&file_path);

        if let Some(ref gs) = gsettings {
            if file_existed {
                push_to_gsettings(gs, &settings);
            } else {
                pull_from_gsettings(gs, &mut settings);
            }
        }

        Self {
            inner: Rc::new(RefCell::new(SettingsStoreInner {
                file_path,
                settings,
                gsettings,
                next_listener_id: 1,
                listeners: Vec::new(),
            })),
        }
    }

    pub fn snapshot(&self) -> AppSettings {
        self.inner.borrow().settings.clone()
    }

    pub fn with<R>(&self, f: impl FnOnce(&AppSettings) -> R) -> R {
        let borrow = self.inner.borrow();
        f(&borrow.settings)
    }

    pub fn update(&self, mutator: impl FnOnce(&mut AppSettings)) {
        let snapshot = {
            let mut inner = self.inner.borrow_mut();
            mutator(&mut inner.settings);
            if let Err(err) = inner.settings.save(&inner.file_path) {
                search_core::warn!(
                    "No se pudieron guardar ajustes en '{}': {err}",
                    inner.file_path.display()
                );
            }
            if let Some(ref gs) = inner.gsettings {
                push_to_gsettings(gs, &inner.settings);
                gio::Settings::sync();
            }
            inner.settings.clone()
        };

        let listeners: Vec<Listener> = self
            .inner
            .borrow()
            .listeners
            .iter()
            .map(|(_, l)| l.clone())
            .collect();
        for listener in listeners {
            listener(&snapshot);
        }
    }

    pub fn subscribe(&self, listener: impl Fn(&AppSettings) + 'static) -> u64 {
        let mut inner = self.inner.borrow_mut();
        let id = inner.next_listener_id;
        inner.next_listener_id += 1;
        inner.listeners.push((id, Rc::new(listener)));
        id
    }

    pub fn unsubscribe(&self, id: u64) {
        self.inner
            .borrow_mut()
            .listeners
            .retain(|(lid, _)| *lid != id);
    }
}

fn find_local_schema_dir() -> Option<PathBuf> {
    let cwd_data = Path::new("data");
    if cwd_data.join("gschemas.compiled").is_file() {
        return Some(cwd_data.to_path_buf());
    }
    let exe = std::env::current_exe().ok()?;
    for ancestor in exe.ancestors().take(4) {
        let candidate = ancestor.join("data");
        if candidate.join("gschemas.compiled").is_file() {
            return Some(candidate);
        }
    }
    None
}

fn pull_from_gsettings(gs: &gio::Settings, out: &mut AppSettings) {
    if let Some(v) = gs.string("search-engine") {
        out.search_engine = SearchEngine::from_id(&v);
    }
    if let Some(v) = gs.string("custom-search-url") {
        out.custom_search_url = v;
    }
    if let Some(v) = gs.string("color-scheme") {
        out.look = ColorLook::from_id(&v);
    }
    if let Some(v) = gs.boolean("block-third-party-cookies") {
        out.set_block_third_party_cookies(v);
    }
    if let Some(v) = gs.boolean("send-gpc") {
        out.send_gpc = v;
    }
    if let Some(v) = gs.boolean("strip-click-ids") {
        out.strip_click_ids = v;
    }
    if let Some(v) = gs.double("page-zoom") {
        out.page_zoom = v;
    }
    if let Some(v) = gs.boolean("hardware-video-decode") {
        out.hardware_video_decode = v;
    }
    if let Some(v) = gs.uint("renderer-process-limit") {
        out.renderer_process_limit = v;
    }
    if let Some(v) = gs.boolean("vertical-tabs") {
        out.vertical_tabs = v;
    }
    if let Some(v) = gs.int("sidebar-width") {
        out.sidebar_width = f64::from(v);
    }
    if let Some(v) = gs.string("downloads-dir") {
        out.downloads_dir = v;
    }
    if let Some(v) = gs.boolean("downloads-ask") {
        out.downloads_ask = v;
    }
    if let Some(v) = gs.boolean("shield-enabled") {
        out.shield_enabled = v;
    }
    if let Some(v) = gs.int("reader-font-size") {
        out.reader_font_size = v;
    }
    if let Some(v) = gs.boolean("vault-save") {
        out.vault_save = v;
    }
    if let Some(v) = gs.boolean("vault-fill") {
        out.vault_fill = v;
    }
    if let Some(v) = gs.boolean("bench-enabled") {
        out.bench_enabled = v;
    }
    if let Some(v) = gs.boolean("update-check") {
        out.update_check = v;
    }
    if let Some(v) = gs.boolean("experimental-extensions") {
        out.extensions_enabled = v;
    }
}

fn push_to_gsettings(gs: &gio::Settings, s: &AppSettings) {
    let _ = gs.set_string("search-engine", s.search_engine.as_id());
    let _ = gs.set_string("custom-search-url", &s.custom_search_url);
    let _ = gs.set_string("color-scheme", s.look.as_id());
    let _ = gs.set_boolean("block-third-party-cookies", s.block_third_party_cookies());
    let _ = gs.set_boolean("send-gpc", s.send_gpc);
    let _ = gs.set_boolean("strip-click-ids", s.strip_click_ids);
    let _ = gs.set_double("page-zoom", s.page_zoom);
    let _ = gs.set_boolean("hardware-video-decode", s.hardware_video_decode);
    let _ = gs.set_uint("renderer-process-limit", s.renderer_process_limit);
    let _ = gs.set_boolean("vertical-tabs", s.vertical_tabs);
    let _ = gs.set_int("sidebar-width", s.sidebar_width.round() as i32);
    let _ = gs.set_string("downloads-dir", &s.downloads_dir);
    let _ = gs.set_boolean("downloads-ask", s.downloads_ask);
    let _ = gs.set_boolean("shield-enabled", s.shield_enabled);
    let _ = gs.set_int("reader-font-size", s.reader_font_size);
    let _ = gs.set_boolean("vault-save", s.vault_save);
    let _ = gs.set_boolean("vault-fill", s.vault_fill);
    let _ = gs.set_boolean("bench-enabled", s.bench_enabled);
    let _ = gs.set_boolean("update-check", s.update_check);
    let _ = gs.set_boolean("experimental-extensions", s.extensions_enabled);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Weak;

    #[test]
    fn settings_subscribe_unsubscribe_drops_listener_rc() {
        let dir =
            std::env::temp_dir().join(format!("search-settings-unsub-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let store = SettingsStore::load(dir.join("settings.json"));

        let sentinel = Rc::new(Cell::new(0usize));
        let weak = Rc::downgrade(&sentinel);
        let sub_id = store.subscribe(move |_| {
            sentinel.set(sentinel.get() + 1);
        });
        assert_eq!(Weak::strong_count(&weak), 1);

        store.unsubscribe(sub_id);
        assert_eq!(Weak::strong_count(&weak), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
