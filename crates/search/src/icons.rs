//! Renderizador propio de iconos vectoriales nítidos en Rust puro (`GdkMemoryTexture`).
//!
//! Cero dependencia de `librsvg`, `gdk-pixbuf`, `glycin` ni temas simbólicos de GNOME/Breeze:
//! cada icono se rasteriza a 2× HiDPI (`32×32` píxeles físicos para `16×16` lógicos) mediante
//! campos de distancia con signo (SDF) y antialiasing analítico, adaptándose automáticamente
//! al modo claro y oscuro de `AdwStyleManager`.

use gnome_platform::adw::StyleManager;
use gnome_platform::gdk::{MemoryFormat, MemoryTexture, Texture};
use gnome_platform::gio::Icon;
use gnome_platform::glib::Bytes;
use gnome_platform::gtk::{Align, AsWidget, Button, Image, Label};
use std::cell::RefCell;
use std::collections::HashMap;

const TEX_SIZE: i32 = 32;

/// Catálogo completo de iconos de interfaz usados en Search Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconId {
    Plus,
    Close,
    Refresh,
    ArrowUp,
    ArrowDown,
    ChevronRight,
    ChevronDown,
    Pin,
    Globe,
    LayoutTabs,
    Folder,
    FolderOpen,
    FolderPlus,
    Star,
    History,
    VolumeHigh,
    VolumeMuted,
    Download,
    FileOpen,
    Trash,
    Edit,
    Undo,
    Stop,
    Warning,
    MoreVert,
    ClearAll,
    Search,
    Check,
    Settings,
    Palette,
    Shield,
    Cpu,
    Key,
    Keyboard,
    Info,
}

thread_local! {
    static ICON_CACHE: RefCell<HashMap<(IconId, bool), Texture>> = RefCell::new(HashMap::new());
}

/// Devuelve `true` si la interfaz está actualmente en modo oscuro.
pub fn is_dark_theme() -> bool {
    StyleManager::default().is_dark()
}

/// Devuelve la textura `GdkTexture` cacheada (32×32 HiDPI) para `id` en tema claro u oscuro.
pub fn texture(id: IconId, dark: bool) -> Texture {
    ICON_CACHE.with(|cache| {
        let mut map = cache.borrow_mut();
        if let Some(tex) = map.get(&(id, dark)) {
            return tex.clone();
        }
        let tex = rasterize_icon(id, dark);
        map.insert((id, dark), tex.clone());
        tex
    })
}

/// Devuelve un `GIcon` respaldado directamente por `GdkTexture` (sin `gdk-pixbuf`/`glycin`).
pub fn gicon(id: IconId, dark: bool) -> Icon {
    texture(id, dark).to_gicon()
}

/// Crea un `GtkImage` con icono vectorial nítido adaptado al tema actual.
pub fn image(id: IconId, pixel_size: i32) -> Image {
    image_for_theme(id, pixel_size, is_dark_theme())
}

/// Crea un `GtkImage` con icono vectorial nítido para el tema indicado.
pub fn image_for_theme(id: IconId, pixel_size: i32, dark: bool) -> Image {
    let img = Image::new();
    let tex = texture(id, dark);
    img.set_from_texture(Some(&tex));
    img.set_pixel_size(pixel_size);
    img
}

/// Actualiza el icono de un `GtkImage` existente al tema actual.
pub fn set_image_icon(img: &Image, id: IconId, dark: bool) {
    let tex = texture(id, dark);
    img.set_from_texture(Some(&tex));
}

/// Crea un botón plano con icono vectorial nítido adaptado al tema actual.
pub fn icon_button(id: IconId, pixel_size: i32) -> Button {
    icon_button_for_theme(id, pixel_size, is_dark_theme())
}

/// Crea un botón plano con icono vectorial nítido para el tema indicado.
pub fn icon_button_for_theme(id: IconId, pixel_size: i32, dark: bool) -> Button {
    let btn = Button::new();
    btn.add_css_class("flat");
    let img = image_for_theme(id, pixel_size, dark);
    img.set_halign(Align::Center);
    img.set_valign(Align::Center);
    btn.set_child(Some(&img));
    btn
}

/// Calcula una inicial limpia (monograma) a partir del dominio o título de una pestaña.
pub fn monogram_letter(title: &str, url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(url) {
        if let Some(host) = parsed.host_str() {
            let bare = host.strip_prefix("www.").unwrap_or(host);
            if let Some(ch) = bare.chars().find(|c| c.is_alphanumeric()) {
                return ch.to_uppercase().to_string();
            }
        }
    }
    if let Some(ch) = title.trim().chars().find(|c| c.is_alphanumeric()) {
        return ch.to_uppercase().to_string();
    }
    "•".to_string()
}

/// Crea una insignia de monograma estilizada para pestañas sin favicon (estilo `Design.swift`).
pub fn monogram_badge(letter: &str, url: &str, large: bool) -> Label {
    let lbl = Label::new(Some(letter));
    lbl.add_css_class("search-monogram");
    if large {
        lbl.add_css_class("large");
    }
    let palette_idx = hash_tint_index(url);
    lbl.add_css_class(&format!("tint-{palette_idx}"));
    lbl.set_halign(Align::Center);
    lbl.set_valign(Align::Center);
    lbl
}

fn hash_tint_index(seed: &str) -> usize {
    let mut h: u32 = 2166136261;
    for b in seed.bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(16777619);
    }
    (h as usize) % 6
}

// ============================================================================
// Motor ligero de rasterización SDF 32×32 (espacio lógico 16×16)
// ============================================================================

struct Canvas {
    size: usize,
    alpha: Vec<f32>,
}

impl Canvas {
    fn new(size: usize) -> Self {
        Self {
            size,
            alpha: vec![0.0; size * size],
        }
    }

    /// Combina cobertura con el píxel usando el operador `max` (ideal para trazos unidos sin costuras).
    fn blend_sdf(&mut self, mut dist_fn: impl FnMut(f32, f32) -> f32) {
        let scale = 16.0 / (self.size as f32);
        // Radio de suavizado ~0.55 px físicos para bordes nítidos sin dientes de sierra.
        let aa = 0.58 * scale;
        for py in 0..self.size {
            let y = (py as f32 + 0.5) * scale;
            let row = py * self.size;
            for px in 0..self.size {
                let x = (px as f32 + 0.5) * scale;
                let d = dist_fn(x, y);
                if d < aa {
                    let cov = ((aa - d) / (2.0 * aa)).clamp(0.0, 1.0);
                    let cur = self.alpha[row + px];
                    if cov > cur {
                        self.alpha[row + px] = cov;
                    }
                }
            }
        }
    }

    fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, stroke: f32) {
        let r = stroke * 0.5;
        self.blend_sdf(|x, y| sd_segment(x, y, x1, y1, x2, y2) - r);
    }

    fn polyline(&mut self, pts: &[(f32, f32)], stroke: f32) {
        for pair in pts.windows(2) {
            self.line(pair[0].0, pair[0].1, pair[1].0, pair[1].1, stroke);
        }
    }

    fn circle_stroke(&mut self, cx: f32, cy: f32, radius: f32, stroke: f32) {
        let half = stroke * 0.5;
        self.blend_sdf(|x, y| ((x - cx).hypot(y - cy) - radius).abs() - half);
    }

    fn circle_fill(&mut self, cx: f32, cy: f32, radius: f32) {
        self.blend_sdf(|x, y| (x - cx).hypot(y - cy) - radius);
    }

    fn rounded_rect_stroke(
        &mut self,
        x0: f32,
        y0: f32,
        w: f32,
        h: f32,
        corner_r: f32,
        stroke: f32,
    ) {
        let cx = x0 + w * 0.5;
        let cy = y0 + h * 0.5;
        let bx = w * 0.5 - corner_r;
        let by = h * 0.5 - corner_r;
        let half = stroke * 0.5;
        self.blend_sdf(|x, y| {
            let d = sd_rounded_box(x - cx, y - cy, bx, by, corner_r);
            d.abs() - half
        });
    }

    fn rounded_rect_fill(&mut self, x0: f32, y0: f32, w: f32, h: f32, corner_r: f32) {
        let cx = x0 + w * 0.5;
        let cy = y0 + h * 0.5;
        let bx = w * 0.5 - corner_r;
        let by = h * 0.5 - corner_r;
        self.blend_sdf(|x, y| sd_rounded_box(x - cx, y - cy, bx, by, corner_r));
    }

    fn arc_stroke(
        &mut self,
        cx: f32,
        cy: f32,
        radius: f32,
        start_deg: f32,
        end_deg: f32,
        stroke: f32,
    ) {
        let steps = 24usize;
        let mut pts = Vec::with_capacity(steps + 1);
        let a0 = start_deg.to_radians();
        let a1 = end_deg.to_radians();
        for i in 0..=steps {
            let t = (i as f32) / (steps as f32);
            let a = a0 + (a1 - a0) * t;
            pts.push((cx + radius * a.cos(), cy + radius * a.sin()));
        }
        self.polyline(&pts, stroke);
    }

    fn into_texture(self, r: u8, g: u8, b: u8, base_alpha: f32) -> Texture {
        let mut bgra = vec![0u8; self.size * self.size * 4];
        for (i, &cov) in self.alpha.iter().enumerate() {
            let a = (cov * base_alpha).clamp(0.0, 1.0);
            let a_u8 = (a * 255.0).round() as u8;
            let base = i * 4;
            bgra[base] = (f32::from(b) * a).round() as u8;
            bgra[base + 1] = (f32::from(g) * a).round() as u8;
            bgra[base + 2] = (f32::from(r) * a).round() as u8;
            bgra[base + 3] = a_u8;
        }
        let gbytes = Bytes::from_slice(&bgra);
        let mem = MemoryTexture::new(
            self.size as i32,
            self.size as i32,
            MemoryFormat::B8g8r8a8Premultiplied,
            &gbytes,
            self.size * 4,
        );
        mem.to_texture()
    }
}

fn sd_segment(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let pax = px - ax;
    let pay = py - ay;
    let bax = bx - ax;
    let bay = by - ay;
    let denom = bax * bax + bay * bay;
    let h = if denom > 1e-6 {
        ((pax * bax + pay * bay) / denom).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (pax - bax * h).hypot(pay - bay * h)
}

fn sd_rounded_box(px: f32, py: f32, bx: f32, by: f32, r: f32) -> f32 {
    let qx = px.abs() - bx;
    let qy = py.abs() - by;
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r
}

fn rasterize_icon(id: IconId, dark: bool) -> Texture {
    let mut c = Canvas::new(TEX_SIZE as usize);
    let sw = 1.45;

    match id {
        IconId::Plus => {
            c.line(8.0, 3.0, 8.0, 13.0, 1.55);
            c.line(3.0, 8.0, 13.0, 8.0, 1.55);
        }
        IconId::Close => {
            c.line(4.0, 4.0, 12.0, 12.0, 1.55);
            c.line(12.0, 4.0, 4.0, 12.0, 1.55);
        }
        IconId::Refresh => {
            c.arc_stroke(8.0, 8.0, 4.8, -50.0, 265.0, sw);
            c.polyline(&[(10.2, 2.6), (11.5, 4.9), (9.0, 5.4)], sw);
        }
        IconId::ArrowUp => {
            c.polyline(&[(3.8, 10.0), (8.0, 5.6), (12.2, 10.0)], 1.55);
        }
        IconId::ArrowDown => {
            c.polyline(&[(3.8, 6.0), (8.0, 10.4), (12.2, 6.0)], 1.55);
        }
        IconId::ChevronRight => {
            c.polyline(&[(6.0, 3.8), (10.3, 8.0), (6.0, 12.2)], 1.6);
        }
        IconId::ChevronDown => {
            c.polyline(&[(3.8, 6.0), (8.0, 10.3), (12.2, 6.0)], 1.6);
        }
        IconId::Pin => {
            // Chincheta inclinada 45° limpia.
            c.line(8.8, 3.0, 13.0, 7.2, sw);
            c.line(9.3, 4.2, 6.4, 7.1, sw);
            c.line(11.8, 6.7, 8.9, 9.6, sw);
            c.line(5.2, 6.3, 9.7, 10.8, sw);
            c.line(6.8, 9.2, 3.2, 12.8, sw);
        }
        IconId::Globe => {
            c.circle_stroke(8.0, 8.0, 5.6, 1.35);
            c.line(2.6, 8.0, 13.4, 8.0, 1.15);
            c.arc_stroke(11.0, 8.0, 6.3, 132.0, 228.0, 1.15);
            c.arc_stroke(5.0, 8.0, 6.3, -48.0, 48.0, 1.15);
        }
        IconId::LayoutTabs => {
            c.rounded_rect_stroke(2.0, 2.6, 12.0, 10.8, 2.1, 1.35);
            c.line(6.2, 2.8, 6.2, 13.2, 1.3);
            c.line(3.5, 5.4, 4.8, 5.4, 1.15);
            c.line(3.5, 7.8, 4.8, 7.8, 1.15);
            c.line(3.5, 10.2, 4.8, 10.2, 1.15);
        }
        IconId::Folder => {
            c.polyline(
                &[
                    (2.2, 4.4),
                    (2.2, 12.4),
                    (13.8, 12.4),
                    (13.8, 5.6),
                    (8.0, 5.6),
                    (6.6, 3.8),
                    (2.6, 3.8),
                    (2.2, 4.4),
                ],
                1.35,
            );
        }
        IconId::FolderOpen => {
            c.polyline(
                &[
                    (2.2, 12.2),
                    (2.2, 4.0),
                    (6.4, 4.0),
                    (7.8, 5.5),
                    (13.2, 5.5),
                    (13.2, 7.4),
                ],
                1.3,
            );
            c.polyline(
                &[
                    (2.2, 12.2),
                    (3.6, 7.6),
                    (14.0, 7.6),
                    (12.6, 12.2),
                    (2.2, 12.2),
                ],
                1.3,
            );
        }
        IconId::FolderPlus => {
            c.polyline(
                &[
                    (2.0, 4.4),
                    (2.0, 12.4),
                    (14.0, 12.4),
                    (14.0, 5.6),
                    (8.2, 5.6),
                    (6.8, 3.8),
                    (2.6, 3.8),
                    (2.0, 4.4),
                ],
                1.3,
            );
            c.line(8.0, 7.4, 8.0, 10.8, 1.3);
            c.line(6.3, 9.1, 9.7, 9.1, 1.3);
        }
        IconId::Star => {
            let mut pts = Vec::with_capacity(11);
            for i in 0..=10 {
                let ang = (-90.0 + (i as f32) * 36.0).to_radians();
                let rad = if i % 2 == 0 { 5.7 } else { 2.5 };
                pts.push((8.0 + rad * ang.cos(), 8.3 + rad * ang.sin()));
            }
            c.polyline(&pts, 1.25);
        }
        IconId::History => {
            c.circle_stroke(8.0, 8.0, 5.4, 1.35);
            c.line(8.0, 4.8, 8.0, 8.2, 1.35);
            c.line(8.0, 8.2, 10.6, 9.6, 1.35);
        }
        IconId::VolumeHigh => {
            c.polyline(
                &[
                    (2.4, 6.4),
                    (4.8, 6.4),
                    (7.8, 3.8),
                    (7.8, 12.2),
                    (4.8, 9.6),
                    (2.4, 9.6),
                    (2.4, 6.4),
                ],
                1.3,
            );
            c.arc_stroke(8.2, 8.0, 2.7, -48.0, 48.0, 1.25);
            c.arc_stroke(8.2, 8.0, 5.0, -52.0, 52.0, 1.25);
        }
        IconId::VolumeMuted => {
            c.polyline(
                &[
                    (2.2, 6.4),
                    (4.5, 6.4),
                    (7.3, 3.8),
                    (7.3, 12.2),
                    (4.5, 9.6),
                    (2.2, 9.6),
                    (2.2, 6.4),
                ],
                1.3,
            );
            c.line(9.8, 5.8, 13.8, 10.2, 1.45);
            c.line(13.8, 5.8, 9.8, 10.2, 1.45);
        }
        IconId::Download => {
            c.line(8.0, 2.8, 8.0, 10.2, sw);
            c.polyline(&[(5.0, 7.6), (8.0, 10.6), (11.0, 7.6)], sw);
            c.polyline(&[(3.0, 11.4), (3.0, 13.0), (13.0, 13.0), (13.0, 11.4)], sw);
        }
        IconId::FileOpen => {
            c.rounded_rect_stroke(2.8, 3.4, 9.2, 9.8, 1.6, 1.3);
            c.line(8.5, 7.5, 13.4, 2.6, 1.35);
            c.polyline(&[(10.0, 2.6), (13.4, 2.6), (13.4, 6.0)], 1.35);
        }
        IconId::Trash => {
            c.line(3.2, 4.4, 12.8, 4.4, 1.35);
            c.line(6.2, 2.6, 9.8, 2.6, 1.35);
            c.polyline(&[(4.4, 4.6), (5.0, 13.2), (11.0, 13.2), (11.6, 4.6)], 1.35);
            c.line(6.8, 6.8, 6.8, 11.0, 1.15);
            c.line(9.2, 6.8, 9.2, 11.0, 1.15);
        }
        IconId::Edit => {
            c.polyline(
                &[
                    (3.0, 13.0),
                    (3.8, 9.6),
                    (10.6, 2.8),
                    (13.2, 5.4),
                    (6.4, 12.2),
                    (3.0, 13.0),
                ],
                1.3,
            );
        }
        IconId::Undo => {
            c.polyline(&[(5.6, 3.6), (2.8, 6.4), (5.6, 9.2)], 1.4);
            c.line(3.2, 6.4, 9.6, 6.4, 1.4);
            c.arc_stroke(9.6, 9.5, 3.1, -90.0, 90.0, 1.4);
            c.line(9.6, 12.6, 5.8, 12.6, 1.4);
        }
        IconId::Stop => {
            c.circle_stroke(8.0, 8.0, 5.4, 1.4);
            c.rounded_rect_fill(5.8, 5.8, 4.4, 4.4, 0.8);
        }
        IconId::Warning => {
            c.polyline(&[(8.0, 2.6), (13.6, 12.8), (2.4, 12.8), (8.0, 2.6)], 1.35);
            c.line(8.0, 6.0, 8.0, 9.4, 1.4);
            c.circle_fill(8.0, 11.1, 0.85);
        }
        IconId::MoreVert => {
            c.circle_fill(8.0, 3.8, 1.25);
            c.circle_fill(8.0, 8.0, 1.25);
            c.circle_fill(8.0, 12.2, 1.25);
        }
        IconId::ClearAll => {
            c.line(3.0, 4.5, 13.0, 4.5, 1.35);
            c.line(3.0, 8.0, 10.6, 8.0, 1.35);
            c.line(3.0, 11.5, 8.2, 11.5, 1.35);
            c.line(10.6, 10.0, 13.4, 12.8, 1.35);
            c.line(13.4, 10.0, 10.6, 12.8, 1.35);
        }
        IconId::Search => {
            c.circle_stroke(7.0, 7.0, 4.1, 1.45);
            c.line(10.1, 10.1, 13.3, 13.3, 1.6);
        }
        IconId::Check => {
            c.polyline(&[(3.2, 8.4), (6.4, 11.6), (12.8, 4.6)], 1.65);
        }
        IconId::Settings => {
            c.circle_stroke(8.0, 8.0, 2.2, 1.4);
            c.circle_stroke(8.0, 8.0, 4.4, 1.25);
            for i in 0..8 {
                let ang = ((i as f32) * 45.0).to_radians();
                let x1 = 8.0 + 4.4 * ang.cos();
                let y1 = 8.0 + 4.4 * ang.sin();
                let x2 = 8.0 + 6.0 * ang.cos();
                let y2 = 8.0 + 6.0 * ang.sin();
                c.line(x1, y1, x2, y2, 1.55);
            }
        }
        IconId::Palette => {
            c.arc_stroke(8.0, 8.0, 5.5, 35.0, 325.0, 1.35);
            c.polyline(&[(12.5, 11.2), (10.0, 11.2), (10.0, 13.3)], 1.3);
            c.circle_fill(6.0, 5.8, 1.0);
            c.circle_fill(9.6, 5.2, 1.0);
            c.circle_fill(5.0, 9.2, 1.0);
        }
        IconId::Shield => {
            c.polyline(
                &[
                    (8.0, 2.2),
                    (13.0, 4.2),
                    (12.6, 8.6),
                    (8.0, 13.6),
                    (3.4, 8.6),
                    (3.0, 4.2),
                    (8.0, 2.2),
                ],
                1.4,
            );
            c.polyline(&[(5.8, 8.1), (7.4, 9.7), (10.4, 6.4)], 1.35);
        }
        IconId::Cpu => {
            c.rounded_rect_stroke(4.4, 4.4, 7.2, 7.2, 1.4, 1.35);
            c.rounded_rect_fill(6.6, 6.6, 2.8, 2.8, 0.5);
            for offset in [6.2, 9.8] {
                c.line(offset, 2.2, offset, 4.2, 1.25);
                c.line(offset, 11.8, offset, 13.8, 1.25);
                c.line(2.2, offset, 4.2, offset, 1.25);
                c.line(11.8, offset, 13.8, offset, 1.25);
            }
        }
        IconId::Key => {
            c.circle_stroke(5.6, 8.0, 2.8, 1.4);
            c.line(8.4, 8.0, 13.6, 8.0, 1.4);
            c.line(11.4, 8.0, 11.4, 10.4, 1.35);
            c.line(13.2, 8.0, 13.2, 9.8, 1.35);
        }
        IconId::Keyboard => {
            c.rounded_rect_stroke(1.8, 4.0, 12.4, 8.0, 1.8, 1.35);
            for x in [4.3, 6.8, 9.3, 11.7] {
                c.circle_fill(x, 6.6, 0.72);
            }
            c.line(4.8, 9.6, 11.2, 9.6, 1.35);
        }
        IconId::Info => {
            c.circle_stroke(8.0, 8.0, 5.6, 1.35);
            c.circle_fill(8.0, 5.2, 0.9);
            c.line(8.0, 7.4, 8.0, 11.2, 1.5);
        }
    }

    if dark {
        c.into_texture(242, 242, 247, 0.90)
    } else {
        c.into_texture(28, 28, 30, 0.86)
    }
}
