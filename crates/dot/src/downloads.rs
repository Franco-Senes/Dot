//! Gestor reactivo de descargas (`Fetching.swift` + `Loot.swift` + `Notifications.swift`):
//! - Conecta `CefDownloadHandler` con el directorio XDG de descargas (`g_get_user_special_dir`)
//!   o `GtkFileDialog::save` si `settings.downloads_ask == true`.
//! - Garantiza que nunca se sobrescribe ningún archivo mediante `unique_download_path`.
//! - Rastrea progreso, bytes recibidos/totales, velocidad actual, cancelación y reintento.
//! - Persiste descargas completadas no privadas en `$XDG_DATA_HOME/search/downloads.json`
//!   con `write_atomic` y debounce de 1 s.
//! - Emite notificación del sistema (`GNotification`) cuando termina una descarga y ninguna
//!   ventana de la aplicación está activa.

use crate::browser_view::{DownloadBeforeEvent, DownloadUpdateEvent};
use crate::settings::SettingsStore;
use cef::{CefString, DownloadItemCallback, ImplBeforeDownloadCallback, ImplDownloadItemCallback};
use dot_core::address::sanitize_url_for_ui;
use dot_core::bookmarks::generate_uuid;
use dot_core::downloads::{
    format_bytes, sanitize_download_filename, unique_download_path, DownloadHistoryStore,
    DownloadRecord,
};
use dot_core::session::now_unix_secs;
use gnome_platform::adw::{Application, ApplicationWindow};
use gnome_platform::gio::{Icon, Notification};
use gnome_platform::glib::{self, SourceId, UserDirectory};
use gnome_platform::gtk::FileDialog;
use std::cell::{Cell, RefCell};
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

const DOWNLOADS_DEBOUNCE_MS: u64 = 1000;

type Listener = Rc<dyn Fn()>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveDownloadStatus {
    InProgress,
    Paused,
    Canceled,
    Interrupted,
}

#[derive(Debug, Clone)]
pub struct ActiveDownloadInfo {
    pub cef_id: u32,
    pub name: String,
    pub path: String,
    pub url: String,
    pub received_bytes: u64,
    pub total_bytes: u64,
    pub percent: Option<f64>,
    pub speed_bps: u64,
    pub status: ActiveDownloadStatus,
    pub is_private: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DownloadsSnapshot {
    pub active: Vec<ActiveDownloadInfo>,
    pub completed: Vec<DownloadRecord>,
}

struct ActiveEntry {
    info: ActiveDownloadInfo,
    callback: Option<DownloadItemCallback>,
    notified_done: bool,
}

struct DownloadManagerInner {
    file_path: PathBuf,
    settings: SettingsStore,
    active: Vec<ActiveEntry>,
    history: DownloadHistoryStore,
    save_debounce: Cell<Option<SourceId>>,
    listeners: Vec<Listener>,
}

#[derive(Clone)]
pub struct DownloadManager {
    inner: Rc<RefCell<DownloadManagerInner>>,
}

impl DownloadManager {
    pub fn load(file_path: PathBuf, settings: SettingsStore) -> Self {
        let history = DownloadHistoryStore::load(&file_path);
        Self {
            inner: Rc::new(RefCell::new(DownloadManagerInner {
                file_path,
                settings,
                active: Vec::new(),
                history,
                save_debounce: Cell::new(None),
                listeners: Vec::new(),
            })),
        }
    }

    /// Devuelve la carpeta de descargas configurada o, por defecto, la carpeta XDG de Descargas.
    pub fn resolve_download_dir(&self) -> PathBuf {
        let configured = self
            .inner
            .borrow()
            .settings
            .with(|s| s.downloads_dir.trim().to_string());
        if !configured.is_empty() {
            return PathBuf::from(configured);
        }
        default_system_downloads_dir()
    }

    pub fn snapshot(&self) -> DownloadsSnapshot {
        let borrow = self.inner.borrow();
        DownloadsSnapshot {
            active: borrow.active.iter().map(|e| e.info.clone()).collect(),
            completed: borrow.history.records().to_vec(),
        }
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) {
        self.inner.borrow_mut().listeners.push(Rc::new(listener));
    }

    /// Atiende `CefDownloadHandler::on_before_download`: decide ruta única o muestra `GtkFileDialog`.
    pub fn handle_before_download(
        &self,
        ev: DownloadBeforeEvent,
        parent: &ApplicationWindow,
        is_private: bool,
    ) {
        let ask = self.inner.borrow().settings.with(|s| s.downloads_ask);
        let dir = self.resolve_download_dir();
        let suggested = sanitize_download_filename(&ev.suggested_name);
        let clean_url = sanitize_url_for_ui(&ev.url);
        let total = ev.total_bytes.max(0) as u64;

        if ask {
            let dialog = FileDialog::new();
            dialog.set_title("Guardar descarga");
            dialog.set_initial_name(&suggested);
            if dir.is_dir() {
                dialog.set_initial_folder(&dir);
            }
            let mgr = self.clone();
            let cb = ev.callback;
            let cef_id = ev.cef_id;
            dialog.save(Some(parent), move |chosen| {
                if let Some(target_path) = chosen {
                    let final_name = target_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(&suggested)
                        .to_string();
                    let full_str = target_path.to_string_lossy().into_owned();
                    mgr.register_starting(
                        cef_id,
                        final_name,
                        full_str.clone(),
                        clean_url,
                        total,
                        is_private,
                    );
                    let cef_path = CefString::from(full_str.as_str());
                    cb.cont(Some(&cef_path), 0);
                }
                // Si el usuario cancela el diálogo, soltar `cb` sin llamar `cont` cancela la descarga en CEF.
            });
            return;
        }

        if let Err(err) = fs::create_dir_all(&dir) {
            dot_core::warn!(
                "No se pudo crear la carpeta de descargas '{}': {err}",
                dir.display()
            );
        }
        let target_path = unique_download_path(&dir, &suggested);
        let final_name = target_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&suggested)
            .to_string();
        let full_str = target_path.to_string_lossy().into_owned();

        self.register_starting(
            ev.cef_id,
            final_name,
            full_str.clone(),
            clean_url,
            total,
            is_private,
        );
        let cef_path = CefString::from(full_str.as_str());
        ev.callback.cont(Some(&cef_path), 0);
    }

    fn register_starting(
        &self,
        cef_id: u32,
        name: String,
        path: String,
        url: String,
        total_bytes: u64,
        is_private: bool,
    ) {
        {
            let mut inner = self.inner.borrow_mut();
            inner.active.retain(|e| e.info.cef_id != cef_id);
            inner.active.insert(
                0,
                ActiveEntry {
                    info: ActiveDownloadInfo {
                        cef_id,
                        name,
                        path,
                        url,
                        received_bytes: 0,
                        total_bytes,
                        percent: if total_bytes > 0 { Some(0.0) } else { None },
                        speed_bps: 0,
                        status: ActiveDownloadStatus::InProgress,
                        is_private,
                    },
                    callback: None,
                    notified_done: false,
                },
            );
        }
        self.notify_listeners();
    }

    /// Atiende `CefDownloadHandler::on_download_updated`: actualiza progreso/velocidad,
    /// persiste al terminar y envía notificación del escritorio si ninguna ventana está activa.
    pub fn handle_download_updated(
        &self,
        ev: DownloadUpdateEvent,
        is_private: bool,
        app: &Application,
        any_window_active: bool,
    ) {
        let mut completed_notification: Option<(String, u64)> = None;
        let mut should_schedule_save = false;

        {
            let mut inner = self.inner.borrow_mut();
            let pos = inner.active.iter().position(|e| e.info.cef_id == ev.cef_id);

            let received = ev.received_bytes.max(0) as u64;
            let total = ev.total_bytes.max(0) as u64;
            let speed = ev.current_speed.max(0) as u64;
            let percent = if ev.percent_complete >= 0 {
                Some((f64::from(ev.percent_complete) / 100.0).clamp(0.0, 1.0))
            } else if total > 0 {
                Some((received as f64 / total as f64).clamp(0.0, 1.0))
            } else {
                None
            };

            let derived_name = if !ev.full_path.is_empty() {
                Path::new(&ev.full_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(String::from)
            } else if !ev.suggested_name.is_empty() {
                Some(sanitize_download_filename(&ev.suggested_name))
            } else {
                None
            };

            if ev.is_complete {
                let mut record_name = derived_name.unwrap_or_else(|| "descarga".to_string());
                let mut record_path = ev.full_path.clone();
                let mut record_url = sanitize_url_for_ui(&ev.url);
                let mut was_private = is_private;
                let mut already_notified = false;

                if let Some(idx) = pos {
                    let existing = inner.active.remove(idx);
                    if record_path.is_empty() {
                        record_path = existing.info.path;
                    }
                    if record_name == "descarga" && !existing.info.name.is_empty() {
                        record_name = existing.info.name;
                    }
                    if record_url.is_empty() {
                        record_url = existing.info.url;
                    }
                    was_private = existing.info.is_private;
                    already_notified = existing.notified_done;
                }

                let final_bytes = total.max(received);
                if !was_private && !record_path.is_empty() {
                    inner.history.add(DownloadRecord {
                        id: generate_uuid(),
                        name: record_name.clone(),
                        path: record_path,
                        url: record_url,
                        bytes: final_bytes,
                        finished: now_unix_secs(),
                    });
                    should_schedule_save = true;
                }

                if !already_notified && !any_window_active {
                    completed_notification = Some((record_name, final_bytes));
                }
            } else {
                let status = if ev.is_canceled {
                    ActiveDownloadStatus::Canceled
                } else if ev.is_interrupted {
                    ActiveDownloadStatus::Interrupted
                } else if ev.is_paused {
                    ActiveDownloadStatus::Paused
                } else {
                    ActiveDownloadStatus::InProgress
                };

                if let Some(idx) = pos {
                    let entry = &mut inner.active[idx];
                    if let Some(n) = derived_name {
                        if !n.is_empty() {
                            entry.info.name = n;
                        }
                    }
                    if !ev.full_path.is_empty() {
                        entry.info.path = ev.full_path;
                    }
                    if !ev.url.is_empty() {
                        entry.info.url = sanitize_url_for_ui(&ev.url);
                    }
                    entry.info.received_bytes = received;
                    entry.info.total_bytes = total;
                    entry.info.percent = percent;
                    entry.info.speed_bps = if status == ActiveDownloadStatus::InProgress {
                        speed
                    } else {
                        0
                    };
                    entry.info.status = status;
                    entry.callback = if status == ActiveDownloadStatus::InProgress
                        || status == ActiveDownloadStatus::Paused
                    {
                        ev.callback
                    } else {
                        None
                    };
                } else if ev.is_in_progress || ev.is_paused {
                    let name = derived_name.unwrap_or_else(|| "descarga".to_string());
                    inner.active.insert(
                        0,
                        ActiveEntry {
                            info: ActiveDownloadInfo {
                                cef_id: ev.cef_id,
                                name,
                                path: ev.full_path,
                                url: sanitize_url_for_ui(&ev.url),
                                received_bytes: received,
                                total_bytes: total,
                                percent,
                                speed_bps: speed,
                                status,
                                is_private,
                            },
                            callback: ev.callback,
                            notified_done: false,
                        },
                    );
                }
            }
        }

        if should_schedule_save {
            self.schedule_save();
        }
        if let Some((filename, size)) = completed_notification {
            send_download_finished_notification(app, &filename, size);
        }
        self.notify_listeners();
    }

    pub fn cancel_download(&self, cef_id: u32) {
        let cb = {
            let mut inner = self.inner.borrow_mut();
            inner
                .active
                .iter_mut()
                .find(|e| e.info.cef_id == cef_id)
                .and_then(|e| {
                    e.info.status = ActiveDownloadStatus::Canceled;
                    e.info.speed_bps = 0;
                    e.callback.take()
                })
        };
        if let Some(callback) = cb {
            callback.cancel();
        }
        self.notify_listeners();
    }

    pub fn dismiss_active(&self, cef_id: u32) {
        self.inner
            .borrow_mut()
            .active
            .retain(|e| e.info.cef_id != cef_id);
        self.notify_listeners();
    }

    pub fn remove_history(&self, id: &str) {
        let changed = self.inner.borrow_mut().history.remove(id);
        if changed {
            self.schedule_save();
            self.notify_listeners();
        }
    }

    pub fn clear_history(&self) {
        {
            let mut inner = self.inner.borrow_mut();
            inner.history.clear();
            inner
                .active
                .retain(|e| e.info.status == ActiveDownloadStatus::InProgress);
        }
        self.schedule_save();
        self.notify_listeners();
    }

    pub fn flush_now(&self) {
        let (path, history) = {
            let inner = self.inner.borrow();
            if let Some(prev) = inner.save_debounce.take() {
                prev.remove();
            }
            (inner.file_path.clone(), inner.history.clone())
        };
        if let Err(err) = history.save(&path) {
            dot_core::warn!(
                "No se pudo guardar el historial de descargas en '{}': {err:#}",
                path.display()
            );
        }
    }

    fn schedule_save(&self) {
        {
            let inner = self.inner.borrow();
            if let Some(prev) = inner.save_debounce.take() {
                prev.remove();
            }
        }
        let mgr = self.clone();
        let src =
            glib::timeout_add_local_once(Duration::from_millis(DOWNLOADS_DEBOUNCE_MS), move || {
                mgr.inner.borrow().save_debounce.set(None);
                mgr.flush_now();
            });
        self.inner.borrow().save_debounce.set(Some(src));
    }

    fn notify_listeners(&self) {
        let listeners: Vec<Listener> = self.inner.borrow().listeners.clone();
        for cb in listeners {
            cb();
        }
    }
}

/// Resuelve la carpeta XDG estándar de descargas del usuario (`G_USER_DIRECTORY_DOWNLOAD`),
/// con respaldo a `$HOME/Downloads` y `$HOME`.
pub fn default_system_downloads_dir() -> PathBuf {
    if let Some(dir) = glib::user_special_dir(UserDirectory::Download) {
        if !dir.as_os_str().is_empty() {
            return dir;
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let h = PathBuf::from(home);
        let dl = h.join("Downloads");
        if dl.is_dir() {
            return dl;
        }
        return h;
    }
    std::env::temp_dir()
}

fn send_download_finished_notification(app: &Application, filename: &str, bytes: u64) {
    let notif = Notification::new("Descarga completada");
    let body = if bytes > 0 {
        format!("{filename} ({})", format_bytes(bytes))
    } else {
        filename.to_string()
    };
    notif.set_body(Some(&body));
    notif.set_icon(&Icon::themed("folder-download-symbolic"));
    app.send_notification(Some("download-finished"), &notif);
}
