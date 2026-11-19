//! Panel lateral discreto de Descargas (`Ctrl+Shift+Y` / `Ctrl+Shift+J`) alojado en
//! `AdwOverlaySplitView` (`Recall.swift` + `DownloadsFloat.swift`).
//!
//! Muestra descargas en curso (progreso, bytes, velocidad, cancelar, reintentar) y
//! completadas (abrir con `GtkFileLauncher`, mostrar en carpeta, reintentar y quitar).

use crate::downloads::{ActiveDownloadInfo, ActiveDownloadStatus, DownloadManager};
use crate::icons::{self, IconId};
use gnome_platform::adw::ApplicationWindow;
use gnome_platform::gtk::{
    Align, AsWidget, BoxWidget, Button, EllipsizeMode, FileLauncher, Label, ListBox, ListBoxRow,
    Orientation, PolicyType, ProgressBar, ScrolledWindow, SelectionMode, Separator,
};
use search_core::address::display_at_rest;
use search_core::downloads::{format_bytes, format_speed, DownloadRecord};
use std::path::Path;
use std::rc::Rc;

type RetryCallback = Rc<dyn Fn(String)>;
type CloseCallback = Rc<dyn Fn()>;

#[derive(Clone)]
pub struct DownloadsPanel {
    root: BoxWidget,
    list_box: ListBox,
    empty_label: Label,
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

        let clear_btn = Button::with_label("Limpiar");
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
        close_btn.set_tooltip_text(Some("Cerrar panel (Esc)"));
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

        let empty_label = Label::new(Some("No hay descargas recientes"));
        empty_label.add_css_class("search-panel-empty");
        empty_label.set_margins(16, 16, 28, 16);
        empty_label.set_halign(Align::Center);
        empty_label.set_valign(Align::Start);

        let list_container = BoxWidget::new(Orientation::Vertical, 0);
        list_container.append(&empty_label);
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
            empty_label,
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
        self.empty_label.set_visible(is_empty);

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

        row.set_child(Some(&hbox));
        row
    }
}
