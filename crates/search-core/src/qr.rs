//! Generador puro en Rust de códigos QR (modo byte, versiones 1 a 10, nivel ECC L)
//! con corrección de errores Reed-Solomon en GF(256) y salida a matriz y SVG.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

/// Matriz booleana de módulos de un código QR (`true` = módulo oscuro).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QrMatrix {
    pub version: u8,
    pub size: usize,
    pub modules: Vec<bool>,
}

impl QrMatrix {
    /// Obtiene el valor del módulo en `(row, col)`.
    pub fn get(&self, row: usize, col: usize) -> bool {
        if row >= self.size || col >= self.size {
            return false;
        }
        self.modules[row * self.size + col]
    }

    /// Renderiza la matriz como un SVG limpio con zona silenciosa (`quiet_zone`) configurable.
    pub fn to_svg(&self, module_px: u32, quiet_zone: u32, fg_hex: &str, bg_hex: &str) -> String {
        let scale = module_px.max(1);
        let total_modules = self.size as u32 + quiet_zone.saturating_mul(2);
        let dim = total_modules * scale;

        let mut path_data = String::new();
        for r in 0..self.size {
            for c in 0..self.size {
                if self.get(r, c) {
                    let x = (c as u32 + quiet_zone) * scale;
                    let y = (r as u32 + quiet_zone) * scale;
                    let _ = write!(path_data, "M{x},{y}h{scale}v{scale}h-{scale}z");
                }
            }
        }

        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {dim} {dim}\" width=\"{dim}\" height=\"{dim}\" shape-rendering=\"crispEdges\"><rect width=\"100%\" height=\"100%\" fill=\"{bg_hex}\"/><path d=\"{path_data}\" fill=\"{fg_hex}\"/></svg>"
        )
    }
}

#[derive(Clone, Copy)]
struct VersionSpec {
    version: u8,
    ec_cw_per_block: usize,
    g1_blocks: usize,
    g1_data_cw: usize,
    g2_blocks: usize,
    g2_data_cw: usize,
}

impl VersionSpec {
    fn total_data_cw(self) -> usize {
        self.g1_blocks * self.g1_data_cw + self.g2_blocks * self.g2_data_cw
    }

    fn char_count_bits(self) -> usize {
        if self.version <= 9 {
            8
        } else {
            16
        }
    }

    fn max_byte_capacity(self) -> usize {
        let total_bits = self.total_data_cw() * 8;
        let overhead = 4 + self.char_count_bits();
        total_bits.saturating_sub(overhead) / 8
    }
}

// Tabla oficial ISO/IEC 18004 para nivel de corrección L (versiones 1..=10).
const VERSION_SPECS_L: &[VersionSpec] = &[
    VersionSpec {
        version: 1,
        ec_cw_per_block: 7,
        g1_blocks: 1,
        g1_data_cw: 19,
        g2_blocks: 0,
        g2_data_cw: 0,
    },
    VersionSpec {
        version: 2,
        ec_cw_per_block: 10,
        g1_blocks: 1,
        g1_data_cw: 34,
        g2_blocks: 0,
        g2_data_cw: 0,
    },
    VersionSpec {
        version: 3,
        ec_cw_per_block: 15,
        g1_blocks: 1,
        g1_data_cw: 55,
        g2_blocks: 0,
        g2_data_cw: 0,
    },
    VersionSpec {
        version: 4,
        ec_cw_per_block: 20,
        g1_blocks: 1,
        g1_data_cw: 80,
        g2_blocks: 0,
        g2_data_cw: 0,
    },
    VersionSpec {
        version: 5,
        ec_cw_per_block: 26,
        g1_blocks: 1,
        g1_data_cw: 108,
        g2_blocks: 0,
        g2_data_cw: 0,
    },
    VersionSpec {
        version: 6,
        ec_cw_per_block: 18,
        g1_blocks: 2,
        g1_data_cw: 68,
        g2_blocks: 0,
        g2_data_cw: 0,
    },
    VersionSpec {
        version: 7,
        ec_cw_per_block: 20,
        g1_blocks: 2,
        g1_data_cw: 78,
        g2_blocks: 0,
        g2_data_cw: 0,
    },
    VersionSpec {
        version: 8,
        ec_cw_per_block: 24,
        g1_blocks: 2,
        g1_data_cw: 97,
        g2_blocks: 0,
        g2_data_cw: 0,
    },
    VersionSpec {
        version: 9,
        ec_cw_per_block: 30,
        g1_blocks: 2,
        g1_data_cw: 116,
        g2_blocks: 0,
        g2_data_cw: 0,
    },
    VersionSpec {
        version: 10,
        ec_cw_per_block: 18,
        g1_blocks: 2,
        g1_data_cw: 68,
        g2_blocks: 2,
        g2_data_cw: 69,
    },
];

fn alignment_centers(version: u8) -> &'static [usize] {
    match version {
        1 => &[],
        2 => &[6, 18],
        3 => &[6, 22],
        4 => &[6, 26],
        5 => &[6, 30],
        6 => &[6, 34],
        7 => &[6, 22, 38],
        8 => &[6, 24, 42],
        9 => &[6, 26, 46],
        10 => &[6, 28, 50],
        _ => &[],
    }
}

/// Codifica texto UTF-8 en una matriz QR (modo byte, ECC L, versiones 1..=10).
pub fn encode_qr_str(text: &str) -> Result<QrMatrix> {
    encode_qr(text.as_bytes())
}

/// Codifica bytes arbitrarios en una matriz QR (modo byte, ECC L, versiones 1..=10).
pub fn encode_qr(data: &[u8]) -> Result<QrMatrix> {
    let spec = VERSION_SPECS_L
        .iter()
        .copied()
        .find(|s| data.len() <= s.max_byte_capacity())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Datos demasiado largos para QR v1..v10 ECC-L ({} bytes)",
                data.len()
            )
        })?;

    let data_codewords = encode_data_codewords(data, spec);
    let interleaved = build_interleaved_codewords(&data_codewords, spec);

    let size = 21 + 4 * (spec.version as usize - 1);
    let mut base_modules = vec![false; size * size];
    let mut is_function = vec![false; size * size];

    draw_function_patterns(spec.version, size, &mut base_modules, &mut is_function);
    place_codewords(&interleaved, size, &mut base_modules, &is_function);

    let mut best_mask = 0u8;
    let mut best_penalty = u32::MAX;
    let mut best_modules = Vec::new();

    for mask in 0..8u8 {
        let mut candidate = base_modules.clone();
        apply_mask(mask, size, &mut candidate, &is_function);
        draw_format_bits(mask, size, &mut candidate);
        let penalty = evaluate_penalty(&candidate, size);
        if penalty < best_penalty {
            best_penalty = penalty;
            best_mask = mask;
            best_modules = candidate;
        }
    }
    let _ = best_mask;

    Ok(QrMatrix {
        version: spec.version,
        size,
        modules: best_modules,
    })
}

fn push_bits(bits: &mut Vec<u8>, value: usize, count: usize) {
    for i in (0..count).rev() {
        bits.push(((value >> i) & 1) as u8);
    }
}

fn encode_data_codewords(data: &[u8], spec: VersionSpec) -> Vec<u8> {
    let mut bits = Vec::with_capacity(spec.total_data_cw() * 8);
    // Modo Byte = 0b0100
    push_bits(&mut bits, 0b0100, 4);
    push_bits(&mut bits, data.len(), spec.char_count_bits());
    for &b in data {
        push_bits(&mut bits, b as usize, 8);
    }

    let capacity_bits = spec.total_data_cw() * 8;
    let terminator = (capacity_bits - bits.len()).min(4);
    push_bits(&mut bits, 0, terminator);
    while bits.len() % 8 != 0 {
        bits.push(0);
    }

    let mut codewords = Vec::with_capacity(spec.total_data_cw());
    for chunk in bits.as_chunks::<8>().0 {
        let mut byte = 0u8;
        for &bit in chunk {
            byte = (byte << 1) | bit;
        }
        codewords.push(byte);
    }

    let pad_bytes = [0xECu8, 0x11u8];
    let mut pad_idx = 0;
    while codewords.len() < spec.total_data_cw() {
        codewords.push(pad_bytes[pad_idx % 2]);
        pad_idx += 1;
    }
    codewords
}

fn gf_mul(mut x: u8, mut y: u8) -> u8 {
    let mut z = 0u8;
    for _ in 0..8 {
        if (y & 1) != 0 {
            z ^= x;
        }
        let carry = x & 0x80;
        x <<= 1;
        if carry != 0 {
            x ^= 0x1d;
        }
        y >>= 1;
    }
    z
}

fn rs_generator_poly(degree: usize) -> Vec<u8> {
    let mut poly = vec![0u8; degree];
    poly[degree - 1] = 1;
    let mut root = 1u8;
    for _ in 0..degree {
        for j in 0..degree {
            poly[j] = gf_mul(poly[j], root);
            if j + 1 < degree {
                poly[j] ^= poly[j + 1];
            }
        }
        root = gf_mul(root, 0x02);
    }
    poly
}

fn rs_compute_remainder(data: &[u8], degree: usize) -> Vec<u8> {
    let gen = rs_generator_poly(degree);
    let mut rem = vec![0u8; degree];
    for &b in data {
        let factor = b ^ rem[0];
        rem.rotate_left(1);
        rem[degree - 1] = 0;
        for (r, &g) in rem.iter_mut().zip(gen.iter()) {
            *r ^= gf_mul(g, factor);
        }
    }
    rem
}

fn build_interleaved_codewords(data_cw: &[u8], spec: VersionSpec) -> Vec<u8> {
    let mut data_blocks: Vec<&[u8]> = Vec::new();
    let mut ec_blocks: Vec<Vec<u8>> = Vec::new();
    let mut offset = 0usize;

    for _ in 0..spec.g1_blocks {
        let block = &data_cw[offset..offset + spec.g1_data_cw];
        offset += spec.g1_data_cw;
        ec_blocks.push(rs_compute_remainder(block, spec.ec_cw_per_block));
        data_blocks.push(block);
    }
    for _ in 0..spec.g2_blocks {
        let block = &data_cw[offset..offset + spec.g2_data_cw];
        offset += spec.g2_data_cw;
        ec_blocks.push(rs_compute_remainder(block, spec.ec_cw_per_block));
        data_blocks.push(block);
    }

    let max_data_len = spec.g1_data_cw.max(spec.g2_data_cw);
    let mut result = Vec::with_capacity(
        spec.total_data_cw() + (spec.g1_blocks + spec.g2_blocks) * spec.ec_cw_per_block,
    );

    for i in 0..max_data_len {
        for block in &data_blocks {
            if i < block.len() {
                result.push(block[i]);
            }
        }
    }
    for i in 0..spec.ec_cw_per_block {
        for ec in &ec_blocks {
            result.push(ec[i]);
        }
    }
    result
}

fn set_func(
    row: usize,
    col: usize,
    val: bool,
    size: usize,
    modules: &mut [bool],
    is_function: &mut [bool],
) {
    let idx = row * size + col;
    modules[idx] = val;
    is_function[idx] = true;
}

fn draw_finder_pattern(
    top: isize,
    left: isize,
    size: usize,
    modules: &mut [bool],
    is_function: &mut [bool],
) {
    for dr in -1..=7 {
        for dc in -1..=7 {
            let r = top + dr;
            let c = left + dc;
            if r >= 0 && r < size as isize && c >= 0 && c < size as isize {
                let dark = if (0..=6).contains(&dr) && (0..=6).contains(&dc) {
                    dr == 0
                        || dr == 6
                        || dc == 0
                        || dc == 6
                        || (2..=4).contains(&dr) && (2..=4).contains(&dc)
                } else {
                    false
                };
                set_func(r as usize, c as usize, dark, size, modules, is_function);
            }
        }
    }
}

fn draw_alignment_pattern(
    center_r: usize,
    center_c: usize,
    size: usize,
    modules: &mut [bool],
    is_function: &mut [bool],
) {
    for dr in -2isize..=2 {
        for dc in -2isize..=2 {
            let r = (center_r as isize + dr) as usize;
            let c = (center_c as isize + dc) as usize;
            let dist = dr.abs().max(dc.abs());
            let dark = dist == 0 || dist == 2;
            set_func(r, c, dark, size, modules, is_function);
        }
    }
}

fn draw_function_patterns(
    version: u8,
    size: usize,
    modules: &mut [bool],
    is_function: &mut [bool],
) {
    // Tres patrones buscadores (esquinas superior-izquierda, superior-derecha, inferior-izquierda).
    draw_finder_pattern(0, 0, size, modules, is_function);
    draw_finder_pattern(0, size as isize - 7, size, modules, is_function);
    draw_finder_pattern(size as isize - 7, 0, size, modules, is_function);

    // Patrones de temporización.
    for i in 8..(size - 8) {
        let dark = i % 2 == 0;
        set_func(6, i, dark, size, modules, is_function);
        set_func(i, 6, dark, size, modules, is_function);
    }

    // Patrones de alineamiento.
    let centers = alignment_centers(version);
    for &r in centers {
        for &c in centers {
            let in_tl = r <= 8 && c <= 8;
            let in_tr = r <= 8 && c >= size - 9;
            let in_bl = r >= size - 9 && c <= 8;
            if !in_tl && !in_tr && !in_bl {
                draw_alignment_pattern(r, c, size, modules, is_function);
            }
        }
    }

    // Módulo oscuro fijo.
    set_func(
        4 * version as usize + 9,
        8,
        true,
        size,
        modules,
        is_function,
    );

    // Reservar áreas de formato (se rellenan tras elegir la máscara).
    for i in 0..9 {
        if i != 6 {
            is_function[8 * size + i] = true;
            is_function[i * size + 8] = true;
        }
    }
    for i in 0..8 {
        is_function[8 * size + (size - 1 - i)] = true;
        is_function[(size - 1 - i) * size + 8] = true;
    }

    // Información de versión para versiones >= 7.
    if version >= 7 {
        let mut rem = version as u32;
        for _ in 0..12 {
            rem = (rem << 1) ^ ((rem >> 11) * 0x1F25);
        }
        let bits = ((version as u32) << 12) | rem;
        for i in 0..18 {
            let bit = ((bits >> i) & 1) != 0;
            let a = size - 11 + (i % 3);
            let b = i / 3;
            set_func(a, b, bit, size, modules, is_function);
            set_func(b, a, bit, size, modules, is_function);
        }
    }
}

fn place_codewords(codewords: &[u8], size: usize, modules: &mut [bool], is_function: &[bool]) {
    let mut bit_idx = 0usize;
    let total_bits = codewords.len() * 8;
    let mut right = size as isize - 1;

    while right >= 1 {
        if right == 6 {
            right -= 1;
        }
        let upward = ((size as isize - 1 - right) / 2) % 2 == 0;
        for step in 0..size {
            let r = if upward { size - 1 - step } else { step };
            for dx in 0..2 {
                let c = (right - dx) as usize;
                let idx = r * size + c;
                if !is_function[idx] {
                    let bit = if bit_idx < total_bits {
                        let byte = codewords[bit_idx / 8];
                        ((byte >> (7 - (bit_idx % 8))) & 1) != 0
                    } else {
                        false
                    };
                    modules[idx] = bit;
                    bit_idx += 1;
                }
            }
        }
        right -= 2;
    }
}

fn apply_mask(mask: u8, size: usize, modules: &mut [bool], is_function: &[bool]) {
    for r in 0..size {
        for c in 0..size {
            let idx = r * size + c;
            if is_function[idx] {
                continue;
            }
            let invert = match mask {
                0 => (r + c) % 2 == 0,
                1 => r % 2 == 0,
                2 => c % 3 == 0,
                3 => (r + c) % 3 == 0,
                4 => (r / 2 + c / 3) % 2 == 0,
                5 => ((r * c) % 2) + ((r * c) % 3) == 0,
                6 => (((r * c) % 2) + ((r * c) % 3)) % 2 == 0,
                7 => (((r + c) % 2) + ((r * c) % 3)) % 2 == 0,
                _ => false,
            };
            if invert {
                modules[idx] = !modules[idx];
            }
        }
    }
}

fn draw_format_bits(mask: u8, size: usize, modules: &mut [bool]) {
    // Nivel L = 0b01; datos de formato = (01 << 3) | mask.
    let data = (0b01u32 << 3) | (mask as u32);
    let mut rem = data;
    for _ in 0..10 {
        rem = (rem << 1) ^ ((rem >> 9) * 0x537);
    }
    let bits = ((data << 10) | rem) ^ 0x5412;

    for i in 0..6 {
        let b = ((bits >> i) & 1) != 0;
        modules[8 * size + i] = b;
    }
    modules[8 * size + 7] = ((bits >> 6) & 1) != 0;
    modules[8 * size + 8] = ((bits >> 7) & 1) != 0;
    modules[7 * size + 8] = ((bits >> 8) & 1) != 0;
    for i in 9..15 {
        modules[(14 - i) * size + 8] = ((bits >> i) & 1) != 0;
    }

    for i in 0..8 {
        modules[8 * size + (size - 1 - i)] = ((bits >> i) & 1) != 0;
    }
    for i in 8..15 {
        modules[(size - 15 + i) * size + 8] = ((bits >> i) & 1) != 0;
    }
}

fn evaluate_penalty(modules: &[bool], size: usize) -> u32 {
    let mut penalty = 0u32;

    // Regla 1: 5 o más módulos consecutivos del mismo color en fila o columna.
    for r in 0..size {
        let mut run_color = modules[r * size];
        let mut run_len = 1u32;
        for c in 1..size {
            let color = modules[r * size + c];
            if color == run_color {
                run_len += 1;
                if run_len == 5 {
                    penalty += 3;
                } else if run_len > 5 {
                    penalty += 1;
                }
            } else {
                run_color = color;
                run_len = 1;
            }
        }
    }
    for c in 0..size {
        let mut run_color = modules[c];
        let mut run_len = 1u32;
        for r in 1..size {
            let color = modules[r * size + c];
            if color == run_color {
                run_len += 1;
                if run_len == 5 {
                    penalty += 3;
                } else if run_len > 5 {
                    penalty += 1;
                }
            } else {
                run_color = color;
                run_len = 1;
            }
        }
    }

    // Regla 2: bloques 2x2 del mismo color.
    for r in 0..(size - 1) {
        for c in 0..(size - 1) {
            let v = modules[r * size + c];
            if modules[r * size + c + 1] == v
                && modules[(r + 1) * size + c] == v
                && modules[(r + 1) * size + c + 1] == v
            {
                penalty += 3;
            }
        }
    }

    // Regla 4: proporción de módulos oscuros cercana al 50%.
    let dark = modules.iter().filter(|&&m| m).count();
    let total = modules.len();
    let pct = (dark * 100) / total;
    let diff = pct.abs_diff(50) / 5;
    penalty += (diff as u32) * 10;

    penalty
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_finder_at(qr: &QrMatrix, top: usize, left: usize) {
        for dr in 0..7 {
            for dc in 0..7 {
                let expected = dr == 0
                    || dr == 6
                    || dc == 0
                    || dc == 6
                    || ((2..=4).contains(&dr) && (2..=4).contains(&dc));
                assert_eq!(
                    qr.get(top + dr, left + dc),
                    expected,
                    "finder mismatch at ({}, {})",
                    top + dr,
                    left + dc
                );
            }
        }
    }

    #[test]
    fn encodes_short_url_as_version_1_with_valid_finders_and_timing() {
        let qr = encode_qr_str("https://rust.org").unwrap();
        assert_eq!(qr.version, 1);
        assert_eq!(qr.size, 21);
        assert_eq!(qr.modules.len(), 21 * 21);

        assert_finder_at(&qr, 0, 0);
        assert_finder_at(&qr, 0, qr.size - 7);
        assert_finder_at(&qr, qr.size - 7, 0);

        // Dark module at (4*v + 9, 8) = (13, 8)
        assert!(qr.get(13, 8));

        // Timing pattern on row 6 and col 6
        for i in 8..(qr.size - 8) {
            assert_eq!(qr.get(6, i), i % 2 == 0);
            assert_eq!(qr.get(i, 6), i % 2 == 0);
        }
    }

    #[test]
    fn scales_to_higher_versions_for_longer_urls() {
        let long_url = format!("https://example.com/{}", "a".repeat(120));
        let qr = encode_qr_str(&long_url).unwrap();
        assert!(qr.version >= 6);
        assert_eq!(qr.size, 21 + 4 * (qr.version as usize - 1));
        assert_finder_at(&qr, 0, 0);
        assert_finder_at(&qr, 0, qr.size - 7);
        assert_finder_at(&qr, qr.size - 7, 0);
    }

    #[test]
    fn renders_clean_svg_and_rejects_oversized_input() {
        let qr = encode_qr_str("hello").unwrap();
        let svg = qr.to_svg(4, 2, "#000000", "#ffffff");
        // (21 + 4) * 4 = 100
        assert!(svg.contains("viewBox=\"0 0 100 100\""));
        assert!(svg.contains("<path d=\"M"));
        assert!(svg.ends_with("</svg>"));

        let too_big = vec![b'x'; 400];
        assert!(encode_qr(&too_big).is_err());
    }
}
