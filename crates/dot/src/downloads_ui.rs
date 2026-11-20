//! Panel lateral discreto de Descargas (`Ctrl+Shift+Y` / `Ctrl+Shift+J`) alojado en
//! `AdwOverlaySplitView` (`Recall.swift` + `DownloadsFloat.swift`).
//!
//! Muestra descargas en curso (progreso, bytes, velocidad, cancelar, reintentar) y
//! completadas (abrir con `GtkFileLauncher`, mostrar en carpeta, reintentar y quitar).

use crate::downloads::{ActiveDownloadInfo, ActiveDownloadStatus, DownloadManager};
use crate::icons::{self, IconId};
use dot_core::address::display_at_rest;
use dot_core::downloads::{format_bytes, format_speed, DownloadRecord};
use gnome_platform::adw::ApplicationWindow;
use gnome_platform::gdk;
use gnome_platform::gtk::{
    Align, AsWidget, BoxWidget, Button, EllipsizeMode, FileLauncher, Label, ListBox, ListBoxRow,
    MenuButton, Orientation, PolicyType, Popover, ProgressBar, ScrolledWindow, SelectionMode,
    Separator,
};
use std::path::{Path, PathBuf};
use std::rc::Rc;

type RetryCallback = Rc<dyn Fn(String)>;
type CloseCallback = Rc<dyn Fn()>;

#[derive(Clone)]
pub struct DownloadsPanel {
    root: BoxWidget,
    list_box: ListBox,
    empty_box: BoxWidget,
    manager: DownloadManager,
    parent_window: ApplicationWindow,
    on_retry: RetryCallback,
}

impl DownloadsPanel {
    pub fn new(
        parent_window: ApplicationWindow,
        manager: DownloadManager,
        on_retry: impl Fn(String) + 'static,
        on_close: impl Fn() + 'static,
    ) -> Self {
        let on_retry: RetryCallback = Rc::new(on_retry);
        let on_close: CloseCallback = Rc::new(on_close);

        let root = BoxWidget::new(Orientation::Vertical, 6);
        root.add_css_class("search-side-panel");
        root.set_hexpand(true);
        root.set_vexpand(true);

        // Cabecera compacta del panel.
        let header = BoxWidget::new(Orientation::Horizontal, 6);
        header.set_margins(8, 8, 10, 4);

        let title = Label::new(Some("Descargas"));
        title.add_css_class("search-panel-title");
        title.set_xalign(0.0);
        title.set_hexpand(true);
        header.append(&title);

        let clear_btn = Button::with_label("Limpiar completadas");
        clear_btn.add_css_class("flat");
        clear_btn.set_tooltip_text(Some("Limpiar lista de descargas completadas"));
        {
            let mgr = manager.clone();
            clear_btn.connect_clicked(move || {
                mgr.clear_history();
            });
        }
        header.append(&clear_btn);

        let close_btn = icons::icon_button(IconId::Close, 15);
        close_btn.set_tooltip_text(Some("Cerrar panel de descargas (Esc)"));
        {
            let cb = on_close.clone();
            close_btn.connect_clicked(move || cb());
        }
        header.append(&close_btn);

        root.append(&header);
        root.append(&Separator::new(Orientation::Horizontal));

        let list_box = ListBox::new();
        list_box.add_css_class("search-panel-list");
        list_box.set_selection_mode(SelectionMode::None);
        list_box.set_activate_on_single_click(false);

        let empty_box = BoxWidget::new(Orientation::Vertical, 8);
        empty_box.set_margins(20, 20, 32, 20);
        empty_box.set_halign(Align::Center);
        empty_box.set_valign(Align::Start);

        let empty_icon = icons::image(IconId::Download, 28);
        empty_icon.set_halign(Align::Center);
        empty_box.append(&empty_icon);

        let empty_title = Label::new(Some("Sin descargas recientes"));
        empty_title.add_css_class("search-panel-title");
        empty_title.set_halign(Align::Center);
        empty_box.append(&empty_title);

        let empty_desc = Label::new(Some(
            "Los archivos que descargues aparecerán aquí con su progreso y ubicación.",
        ));
        empty_desc.add_css_class("search-panel-empty");
        empty_desc.set_wrap(true);
        empty_desc.set_halign(Align::Center);
        empty_box.append(&empty_desc);

        let open_dir_btn = Button::with_label("Abrir carpeta de descargas");
        open_dir_btn.add_css_class("suggested-action");
        open_dir_btn.set_halign(Align::Center);
        {
            let win = parent_window.clone();
            open_dir_btn.connect_clicked(move || {
                let dir = default_downloads_dir();
                FileLauncher::launch_file(Some(&win), &dir);
            });
        }
        empty_box.append(&open_dir_btn);

        let list_container = BoxWidget::new(Orientation::Vertical, 0);
        list_container.append(&empty_box);
        list_container.append(&list_box);

        let scroller = ScrolledWindow::new();
        scroller.set_policy(PolicyType::Never, PolicyType::Automatic);
        scroller.set_hexpand(true);
        scroller.set_vexpand(true);
        scroller.set_child(Some(&list_container));
        root.append(&scroller);

        let panel = Self {
            root,
            list_box,
            empty_box,
            manager: manager.clone(),
            parent_window,
            on_retry,
        };

        {
            let p = panel.clone();
            manager.subscribe(move || {
                p.refresh();
            });
        }

        panel.refresh();
        panel
    }

    pub fn widget(&self) -> &BoxWidget {
        &self.root
    }

    pub fn refresh(&self) {
        self.list_box.remove_all();
        let snap = self.manager.snapshot();

        let is_empty = snap.active.is_empty() && snap.completed.is_empty();
        self.empty_box.set_visible(is_empty);

        for active in &snap.active {
            let row = self.build_active_row(active);
            self.list_box.append(&row);
        }

        for rec in &snap.completed {
            let row = self.build_completed_row(rec);
            self.list_box.append(&row);
        }
    }

    fn build_active_row(&self, item: &ActiveDownloadInfo) -> ListBoxRow {
        let row = ListBoxRow::new();
        row.add_css_class("search-panel-row");
        row.set_selectable(false);
        row.set_activatable(false);

        let vbox = BoxWidget::new(Orientation::Vertical, 4);
        vbox.set_margins(8, 8, 6, 6);

        let top = BoxWidget::new(Orientation::Horizontal, 6);
        let icon = icons::image(IconId::Download, 16);
        top.append(&icon);

        let name_lbl = Label::new(Some(&item.name));
        name_lbl.set_xalign(0.0);
        name_lbl.set_hexpand(true);
        name_lbl.set_ellipsize(EllipsizeMode::Middle);
        top.append(&name_lbl);

        match item.status {
            ActiveDownloadStatus::InProgress | ActiveDownloadStatus::Paused => {
                let cancel_btn = icons::icon_button(IconId::Stop, 14);
                cancel_btn.add_css_class("search-tab-mini-btn");
                cancel_btn.set_tooltip_text(Some("Cancelar descarga"));
                let mgr = self.manager.clone();
                let id = item.cef_id;
                cancel_btn.connect_clicked(move || mgr.cancel_download(id));
                top.append(&cancel_btn);
            }
            ActiveDownloadStatus::Canceled | ActiveDownloadStatus::Interrupted => {
                if !item.url.is_empty() {
                    let retry_btn = icons::icon_button(IconId::Refresh, 14);
                    retry_btn.add_css_class("search-tab-mini-btn");
                    retry_btn.set_tooltip_text(Some("Reintentar descarga"));
                    let retry = self.on_retry.clone();
                    let mgr = self.manager.clone();
                    let id = item.cef_id;
                    let url = item.url.clone();
                    retry_btn.connect_clicked(move || {
                        mgr.dismiss_active(id);
                        retry(url.clone());
                    });
                    top.append(&retry_btn);
                }
                let del_btn = icons::icon_button(IconId::Close, 13);
                del_btn.add_css_class("search-tab-mini-btn");
                del_btn.set_tooltip_text(Some("Quitar de la lista"));
                let mgr = self.manager.clone();
                let id = item.cef_id;
                del_btn.connect_clicked(move || mgr.dismiss_active(id));
                top.append(&del_btn);
            }
        }
        vbox.append(&top);

        if item.status == ActiveDownloadStatus::InProgress
            || item.status == ActiveDownloadStatus::Paused
        {
            let pbar = ProgressBar::new();
            pbar.set_hexpand(true);
            if let Some(frac) = item.percent {
                pbar.set_fraction(frac);
            } else {
                pbar.pulse();
            }
            vbox.append(&pbar);
        }

        let detail_text = match item.status {
            ActiveDownloadStatus::InProgress => {
                let pct = item
                    .percent
                    .map(|p| format!("{} % · ", (p * 100.0).round() as u32))
                    .unwrap_or_default();
                let size_part = if item.total_bytes > 0 {
                    format!(
                        "{} / {}",
                        format_bytes(item.received_bytes),
                        format_bytes(item.total_bytes)
                    )
                } else {
                    format_bytes(item.received_bytes)
                };
                if item.speed_bps > 0 {
                    format!("{pct}{size_part} · {}", format_speed(item.speed_bps))
                } else {
                    format!("{pct}{size_part}")
                }
            }
            ActiveDownloadStatus::Paused => "En pausa".to_string(),
            ActiveDownloadStatus::Canceled => "Cancelada".to_string(),
            ActiveDownloadStatus::Interrupted => "Interrumpida".to_string(),
        };
        let detail = Label::new(Some(&detail_text));
        detail.add_css_class("search-panel-subtitle");
        detail.set_xalign(0.0);
        detail.set_ellipsize(EllipsizeMode::End);
        vbox.append(&detail);

        row.set_child(Some(&vbox));
        row
    }

    fn build_completed_row(&self, rec: &DownloadRecord) -> ListBoxRow {
        let row = ListBoxRow::new();
        row.add_css_class("search-panel-row");
        row.set_selectable(false);
        row.set_activatable(false);

        let hbox = BoxWidget::new(Orientation::Horizontal, 8);
        hbox.set_margins(8, 8, 6, 6);

        let exists = rec.file_exists();
        let icon = icons::image(
            if exists {
                IconId::Download
            } else {
                IconId::Warning
            },
            16,
        );
        hbox.append(&icon);

        let text_col = BoxWidget::new(Orientation::Vertical, 2);
        text_col.set_hexpand(true);

        let name_lbl = Label::new(Some(&rec.name));
        name_lbl.set_xalign(0.0);
        name_lbl.set_ellipsize(EllipsizeMode::Middle);
        text_col.append(&name_lbl);

        let host = display_at_rest(&rec.url);
        let meta_text = if !exists {
            "Archivo movido o eliminado".to_string()
        } else if !host.is_empty() {
            format!("{} · {host}", format_bytes(rec.bytes))
        } else {
            format_bytes(rec.bytes)
        };
        let sub_lbl = Label::new(Some(&meta_text));
        sub_lbl.add_css_class("search-panel-subtitle");
        sub_lbl.set_xalign(0.0);
        sub_lbl.set_ellipsize(EllipsizeMode::End);
        text_col.append(&sub_lbl);
        hbox.append(&text_col);

        if exists {
            let open_btn = icons::icon_button(IconId::FileOpen, 14);
            open_btn.add_css_class("search-tab-mini-btn");
            open_btn.set_tooltip_text(Some("Abrir archivo"));
            let win = self.parent_window.clone();
            let path = rec.path.clone();
            open_btn.connect_clicked(move || {
                FileLauncher::launch_file(Some(&win), Path::new(&path));
            });
            hbox.append(&open_btn);

            let folder_btn = icons::icon_button(IconId::FolderOpen, 14);
            folder_btn.add_css_class("search-tab-mini-btn");
            folder_btn.set_tooltip_text(Some("Mostrar en la carpeta"));
            let win = self.parent_window.clone();
            let path = rec.path.clone();
            folder_btn.connect_clicked(move || {
                FileLauncher::open_containing_folder(Some(&win), Path::new(&path));
            });
            hbox.append(&folder_btn);
        } else if !rec.url.is_empty() {
            let retry_btn = icons::icon_button(IconId::Refresh, 14);
            retry_btn.add_css_class("search-tab-mini-btn");
            retry_btn.set_tooltip_text(Some("Volver a descargar"));
            let retry = self.on_retry.clone();
            let url = rec.url.clone();
            retry_btn.connect_clicked(move || retry(url.clone()));
            hbox.append(&retry_btn);
        }

        let remove_btn = icons::icon_button(IconId::Trash, 14);
        remove_btn.add_css_class("search-tab-mini-btn");
        remove_btn.set_tooltip_text(Some("Quitar de la lista"));
        let mgr = self.manager.clone();
        let id = rec.id.clone();
        remove_btn.connect_clicked(move || mgr.remove_history(&id));
        hbox.append(&remove_btn);

        if !rec.url.is_empty() {
            let more_pop = Popover::new();
            let more_box = BoxWidget::new(Orientation::Vertical, 2);
            more_box.set_margins(4, 4, 4, 4);

            let copy_url_btn = Button::with_label("Copiar URL de descarga");
            copy_url_btn.add_css_class("flat");
            {
                let pop = more_pop.clone();
                let url = rec.url.clone();
                copy_url_btn.connect_clicked(move || {
                    pop.popdown();
                    if let Some(disp) = gdk::Display::default_display() {
                        disp.set_clipboard_text(&url);
                    }
                });
            }
            more_box.append(&copy_url_btn);

            if exists {
                let retry_btn = Button::with_label("Volver a descargar");
                retry_btn.add_css_class("flat");
                let pop = more_pop.clone();
                let retry = self.on_retry.clone();
                let url = rec.url.clone();
                retry_btn.connect_clicked(move || {
                    pop.popdown();
                    retry(url.clone());
                });
                more_box.append(&retry_btn);
            }

            more_pop.set_child(Some(&more_box));
            let more_btn = MenuButton::new();
            more_btn.add_css_class("flat");
            more_btn.add_css_class("search-tab-mini-btn");
            more_btn.set_child(Some(&icons::image(IconId::MoreVert, 14)));
            more_btn.set_tooltip_text(Some("Más acciones de descarga"));
            more_btn.set_popover(Some(&more_pop));
            hbox.append(&more_btn);
        }

        row.set_child(Some(&hbox));
        row
    }
}

fn default_downloads_dir() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        let base = PathBuf::from(home);
        let es = base.join("Descargas");
        if es.is_dir() {
            return es;
        }
        let en = base.join("Downloads");
        if en.is_dir() {
            return en;
        }
        return base;
    }
    PathBuf::from("/tmp")
}
