//! Parser CRX3 seguro, verificación criptográfica RSA/ECDSA y extracción ZIP protegida.
//!
//! Portado de `upstream/Sources/Search/Crx.swift` y `ExtensionFiles.swift` en Rust puro
//! sobre `ring`, `flate2` y `crc32fast`.

use flate2::read::DeflateDecoder;
use ring::digest::{digest, SHA256};
use ring::signature::{self, UnparsedPublicKey};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use url::Url;

/// Versión de Chromium anunciada a Chrome Web Store al descargar paquetes CRX3.
pub const CHROME_VERSION: &str = "156.0.0.0";

/// Tamaño máximo aceptado para el encabezado Protobuf de un CRX3 (4 MiB).
pub const MAX_CRX_HEADER_BYTES: usize = 4 * 1024 * 1024;

/// Número máximo de entradas en un archivo ZIP por defecto.
pub const DEFAULT_MAX_FILES: usize = 4096;

/// Tamaño máximo descomprimido por archivo individual (50 MiB).
pub const DEFAULT_MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;

/// Tamaño máximo descomprimido total de una extensión (200 MiB).
pub const DEFAULT_MAX_TOTAL_BYTES: u64 = 200 * 1024 * 1024;

/// Relación máxima de compresión (uncompressed / compressed) para entradas > 64 KiB.
pub const DEFAULT_MAX_COMPRESSION_RATIO: u64 = 100;

const CRX_MAGIC: &[u8; 4] = b"Cr24";
const CRX_SIGNED_DATA_PREFIX: &[u8] = b"CRX3 SignedData\0";
const MIN_RATIO_CHECK_BYTES: u64 = 64 * 1024;

// OIDs ASN.1 DER
const OID_RSA_ENCRYPTION: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01];
const OID_EC_PUBLIC_KEY: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01];
const OID_PRIME256V1: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07];

static STAGING_SEQ: AtomicU64 = AtomicU64::new(1);

/// Errores de validación, firma y extracción de paquetes CRX3.
#[derive(Debug, Error)]
pub enum CrxError {
    #[error("el identificador o enlace de Chrome Web Store no es válido")]
    InvalidExtensionId,
    #[error("el archivo no es un paquete CRX3 válido: {0}")]
    InvalidCrxFormat(String),
    #[error("encabezado protobuf CRX3 malformado: {0}")]
    InvalidProtobuf(String),
    #[error("la firma o el ID criptográfico de la extensión no coincide: {0}")]
    SignatureVerificationFailed(String),
    #[error("el archivo ZIP interno está dañado o no es soportado: {0}")]
    InvalidZip(String),
    #[error("ruta insegura en el archivo ZIP (path traversal): {0}")]
    UnsafePath(String),
    #[error("no se permiten enlaces simbólicos dentro de una extensión: {0}")]
    SymlinkNotAllowed(String),
    #[error("tipo de entrada no permitido en el archivo ZIP: {0}")]
    UnsupportedEntryType(String),
    #[error("límite de extracción superado ({0})")]
    SizeLimitExceeded(String),
    #[error("posible zip bomb detectada en '{entry}' (relación {ratio}x > {max_ratio}x)")]
    ZipBomb {
        entry: String,
        ratio: u64,
        max_ratio: u64,
    },
    #[error("el CRC-32 de '{0}' no coincide")]
    CrcMismatch(String),
    #[error("falta manifest.json en la raíz de la extensión")]
    MissingManifest,
    #[error("error de E/S al extraer la extensión: {0}")]
    Io(#[from] io::Error),
}

/// Límites de seguridad al extraer un archivo ZIP de extensión.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractLimits {
    pub max_files: usize,
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
    pub max_compression_ratio: u64,
    pub require_manifest: bool,
}

impl Default for ExtractLimits {
    fn default() -> Self {
        Self {
            max_files: DEFAULT_MAX_FILES,
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            max_total_bytes: DEFAULT_MAX_TOTAL_BYTES,
            max_compression_ratio: DEFAULT_MAX_COMPRESSION_RATIO,
            require_manifest: true,
        }
    }
}

/// Par clave-firma (`AsymmetricKeyProof`) extraído del encabezado CRX3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsymmetricKeyProof {
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

/// Encabezado CRX3 ya parseado junto al payload ZIP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCrx3<'a> {
    pub header_bytes: &'a [u8],
    pub signed_header_data: Vec<u8>,
    pub crx_id: String,
    pub rsa_proofs: Vec<AsymmetricKeyProof>,
    pub ecdsa_proofs: Vec<AsymmetricKeyProof>,
    pub zip_payload: &'a [u8],
}

/// Informe de extracción segura de un archivo ZIP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractionReport {
    pub files_extracted: usize,
    pub dirs_created: usize,
    pub total_bytes: u64,
}

/// Comprueba si `id` tiene exactamente 32 letras ASCII en el rango `a..=p`.
pub fn is_valid_extension_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b))
}

/// Convierte cada nibble (4 bits) en una letra de `'a'` (0) a `'p'` (15).
pub fn bytes_to_extension_id(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(char::from(b'a' + (byte >> 4)));
        out.push(char::from(b'a' + (byte & 0x0f)));
    }
    out
}

/// Calcula el ID de extensión de Chrome a partir de una clave pública DER (SPKI):
/// los primeros 16 bytes del SHA-256 codificados en letras `a..=p`.
pub fn extension_id_from_public_key(spki_der: &[u8]) -> String {
    let hash = digest(&SHA256, spki_der);
    bytes_to_extension_id(&hash.as_ref()[..16])
}

/// Calcula el ID determinista que asigna Chrome a una carpeta cargada sin empaquetar
/// a partir de su ruta canónica real.
pub fn unpacked_extension_id(folder: &Path) -> Result<String, CrxError> {
    let canonical = fs::canonicalize(folder)?;
    let path_str = canonical.to_string_lossy();
    let hash = digest(&SHA256, path_str.as_bytes());
    Ok(bytes_to_extension_id(&hash.as_ref()[..16]))
}

/// Extrae un identificador de 32 letras `[a-p]` delimitado desde un texto libre
/// (ID directo, URL de Chrome Web Store nueva o antigua).
pub fn extract_extension_id(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_lowercase() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_lowercase() {
                i += 1;
            }
            let token = &lower[start..i];
            if is_valid_extension_id(token) {
                return Some(token.to_owned());
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Extrae el ID de extensión únicamente si la URL pertenece a una página de detalle
/// oficial de Chrome Web Store sobre HTTPS sin credenciales embebidas.
pub fn store_id_from_url(url: &Url) -> Option<String> {
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    let segments: Vec<&str> = url.path().split('/').filter(|s| !s.is_empty()).collect();

    let after_detail = match host.as_str() {
        "chromewebstore.google.com" => {
            if segments.first().copied() != Some("detail") {
                return None;
            }
            &segments[1..]
        }
        "chrome.google.com" => {
            if segments.get(0..2) != Some(&["webstore", "detail"][..]) {
                return None;
            }
            &segments[2..]
        }
        _ => return None,
    };

    // Admite `/detail/<id>` o `/detail/<slug>/<id>(/...)`.
    if let Some(&first) = after_detail.first() {
        if is_valid_extension_id(first) {
            return Some(first.to_owned());
        }
    }
    if let Some(&second) = after_detail.get(1) {
        if is_valid_extension_id(second) {
            return Some(second.to_owned());
        }
    }
    None
}

/// Construye la URL oficial de descarga CRX3 de Chrome Web Store para el ID indicado.
pub fn cws_download_url(id: &str) -> Result<Url, CrxError> {
    let normalized = id.trim().to_ascii_lowercase();
    if !is_valid_extension_id(&normalized) {
        return Err(CrxError::InvalidExtensionId);
    }

    let mut url = Url::parse("https://clients2.google.com/service/update2/crx")
        .map_err(|_| CrxError::InvalidExtensionId)?;
    url.query_pairs_mut()
        .append_pair("response", "redirect")
        .append_pair("prodversion", CHROME_VERSION)
        .append_pair("acceptformat", "crx3")
        .append_pair("x", &format!("id={normalized}&installsource=ondemand&uc"));
    Ok(url)
}

/// Parsea el contenedor CRX3 y su encabezado Protobuf sin verificar aún las firmas.
pub fn parse_crx3(crx: &[u8]) -> Result<ParsedCrx3<'_>, CrxError> {
    if crx.len() < 12 {
        return Err(CrxError::InvalidCrxFormat(
            "archivo demasiado corto (< 12 bytes)".into(),
        ));
    }
    if &crx[0..4] != CRX_MAGIC {
        return Err(CrxError::InvalidCrxFormat("firma mágica != Cr24".into()));
    }
    let version = read_le_u32(crx, 4)?;
    if version != 3 {
        return Err(CrxError::InvalidCrxFormat(format!(
            "versión CRX no soportada: {version} (se esperaba 3)"
        )));
    }
    let header_size = read_le_u32(crx, 8)? as usize;
    if header_size == 0 || header_size > MAX_CRX_HEADER_BYTES {
        return Err(CrxError::InvalidCrxFormat(format!(
            "tamaño de encabezado inválido: {header_size}"
        )));
    }
    let header_end = 12usize.checked_add(header_size).ok_or_else(|| {
        CrxError::InvalidCrxFormat("desbordamiento en tamaño de encabezado".into())
    })?;
    if header_end > crx.len() {
        return Err(CrxError::InvalidCrxFormat(
            "encabezado CRX3 truncado respecto al tamaño del archivo".into(),
        ));
    }

    let header_bytes = &crx[12..header_end];
    let zip_payload = &crx[header_end..];
    if zip_payload.is_empty() {
        return Err(CrxError::InvalidCrxFormat(
            "el payload ZIP interno está vacío".into(),
        ));
    }

    let fields = parse_protobuf_fields(header_bytes)?;
    let mut rsa_proofs = Vec::new();
    let mut ecdsa_proofs = Vec::new();
    let mut signed_header_data: Option<Vec<u8>> = None;

    for (field_number, value) in fields {
        match field_number {
            2 => rsa_proofs.push(parse_key_proof(value)?),
            3 => ecdsa_proofs.push(parse_key_proof(value)?),
            10000 => signed_header_data = Some(value.to_vec()),
            _ => {}
        }
    }

    let signed_header_data = signed_header_data.ok_or_else(|| {
        CrxError::SignatureVerificationFailed("falta signed_header_data (campo 10000)".into())
    })?;

    let signed_fields = parse_protobuf_fields(&signed_header_data)?;
    let raw_crx_id = signed_fields
        .into_iter()
        .find(|(num, _)| *num == 1)
        .map(|(_, data)| data)
        .ok_or_else(|| {
            CrxError::SignatureVerificationFailed(
                "falta crx_id (campo 1) en signed_header_data".into(),
            )
        })?;
    if raw_crx_id.len() != 16 {
        return Err(CrxError::SignatureVerificationFailed(format!(
            "longitud inválida de crx_id: {} bytes (se esperaban 16)",
            raw_crx_id.len()
        )));
    }

    let crx_id = bytes_to_extension_id(raw_crx_id);
    Ok(ParsedCrx3 {
        header_bytes,
        signed_header_data,
        crx_id,
        rsa_proofs,
        ecdsa_proofs,
        zip_payload,
    })
}

/// Verifica la firma CRX3 (RSA-PKCS1v15-SHA256 o ECDSA-P256-SHA256) y que el ID derivado
/// de la clave pública y el `crx_id` firmado coincidan con `expected_id`.
///
/// Devuelve un slice al payload ZIP verificado.
pub fn verify_crx3<'a>(crx: &'a [u8], expected_id: &str) -> Result<&'a [u8], CrxError> {
    let expected = expected_id.trim().to_ascii_lowercase();
    if !is_valid_extension_id(&expected) {
        return Err(CrxError::InvalidExtensionId);
    }

    let parsed = parse_crx3(crx)?;
    if parsed.crx_id != expected {
        return Err(CrxError::SignatureVerificationFailed(format!(
            "crx_id del paquete ('{}') != esperado ('{}')",
            parsed.crx_id, expected
        )));
    }

    let signed_len = u32::try_from(parsed.signed_header_data.len())
        .map_err(|_| CrxError::InvalidCrxFormat("signed_header_data supera u32::MAX".into()))?;

    let mut signed_message = Vec::with_capacity(
        CRX_SIGNED_DATA_PREFIX.len()
            + 4
            + parsed.signed_header_data.len()
            + parsed.zip_payload.len(),
    );
    signed_message.extend_from_slice(CRX_SIGNED_DATA_PREFIX);
    signed_message.extend_from_slice(&signed_len.to_le_bytes());
    signed_message.extend_from_slice(&parsed.signed_header_data);
    signed_message.extend_from_slice(parsed.zip_payload);

    let mut matched_key = false;

    for proof in &parsed.rsa_proofs {
        if extension_id_from_public_key(&proof.public_key) == expected {
            matched_key = true;
            if verify_rsa_spki_pkcs1_sha256(&proof.public_key, &signed_message, &proof.signature) {
                return Ok(parsed.zip_payload);
            }
        }
    }

    for proof in &parsed.ecdsa_proofs {
        if extension_id_from_public_key(&proof.public_key) == expected {
            matched_key = true;
            if verify_ecdsa_p256_spki_sha256(&proof.public_key, &signed_message, &proof.signature) {
                return Ok(parsed.zip_payload);
            }
        }
    }

    if matched_key {
        Err(CrxError::SignatureVerificationFailed(
            "la firma criptográfica del paquete no es válida".into(),
        ))
    } else {
        Err(CrxError::SignatureVerificationFailed(
            "ninguna clave pública del encabezado coincide con el ID solicitado".into(),
        ))
    }
}

/// Verifica un paquete CRX3 y lo extrae de forma segura y atómica en `destination`.
pub fn verify_and_extract_crx3(
    crx: &[u8],
    expected_id: &str,
    destination: &Path,
    limits: ExtractLimits,
) -> Result<ExtractionReport, CrxError> {
    let zip_bytes = verify_crx3(crx, expected_id)?;
    extract_zip_atomic(zip_bytes, destination, limits)
}

/// Extrae un archivo ZIP en un directorio temporal hermano y sustituye `destination`
/// de forma atómica, rechazando path traversal, enlaces simbólicos y zip bombs.
pub fn extract_zip_atomic(
    zip_bytes: &[u8],
    destination: &Path,
    limits: ExtractLimits,
) -> Result<ExtractionReport, CrxError> {
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

    let seq = STAGING_SEQ.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let base_name = destination
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("extension");

    let staging_dir = parent.join(format!(".{base_name}.{pid}.{nanos}.{seq}.staging"));
    let backup_dir = parent.join(format!(".{base_name}.{pid}.{nanos}.{seq}.old"));

    fs::create_dir_all(&staging_dir)?;

    let report = match extract_zip_into(zip_bytes, &staging_dir, limits) {
        Ok(report) => report,
        Err(err) => {
            let _ = fs::remove_dir_all(&staging_dir);
            return Err(err);
        }
    };

    if let Err(err) = ensure_no_symlinks_on_disk(&staging_dir) {
        let _ = fs::remove_dir_all(&staging_dir);
        return Err(err);
    }

    let had_existing = destination.exists();
    if had_existing {
        if let Err(err) = fs::rename(destination, &backup_dir) {
            let _ = fs::remove_dir_all(&staging_dir);
            return Err(CrxError::Io(err));
        }
    }

    if let Err(err) = fs::rename(&staging_dir, destination) {
        if had_existing {
            let _ = fs::rename(&backup_dir, destination);
        }
        let _ = fs::remove_dir_all(&staging_dir);
        return Err(CrxError::Io(err));
    }

    if had_existing {
        let _ = fs::remove_dir_all(&backup_dir);
    }

    Ok(report)
}

/// Extrae un archivo ZIP en `out_dir` aplicando todas las defensas de seguridad.
pub fn extract_zip_into(
    zip_bytes: &[u8],
    out_dir: &Path,
    limits: ExtractLimits,
) -> Result<ExtractionReport, CrxError> {
    let entries = parse_zip_central_directory(zip_bytes, limits)?;
    fs::create_dir_all(out_dir)?;

    let mut seen_paths = HashSet::with_capacity(entries.len());
    let mut files_extracted = 0usize;
    let mut dirs_created = 0usize;
    let mut total_bytes = 0u64;
    let mut has_manifest = false;

    for entry in entries {
        let rel_path = validate_zip_entry_path(&entry.name)?;
        if !seen_paths.insert(rel_path.clone()) {
            return Err(CrxError::InvalidZip(format!(
                "entrada duplicada en ZIP: '{}'",
                entry.name
            )));
        }

        if rel_path == Path::new("manifest.json") && !entry.is_dir {
            has_manifest = true;
        }

        let target = out_dir.join(&rel_path);
        if entry.is_dir {
            fs::create_dir_all(&target)?;
            dirs_created += 1;
            continue;
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }

        let comp_slice = local_entry_compressed_slice(zip_bytes, &entry)?;
        let remaining_total = limits
            .max_total_bytes
            .checked_sub(total_bytes)
            .ok_or_else(|| CrxError::SizeLimitExceeded("tamaño total excedido".into()))?;
        let entry_cap = limits.max_file_bytes.min(remaining_total);

        let written = match entry.compression_method {
            0 => write_stored_entry(&target, comp_slice, &entry, entry_cap)?,
            8 => write_deflated_entry(&target, comp_slice, &entry, entry_cap, limits)?,
            other => {
                return Err(CrxError::InvalidZip(format!(
                    "método de compresión ZIP no soportado ({other}) en '{}'",
                    entry.name
                )));
            }
        };

        total_bytes = total_bytes
            .checked_add(written)
            .ok_or_else(|| CrxError::SizeLimitExceeded("desbordamiento de tamaño total".into()))?;
        files_extracted += 1;
    }

    if limits.require_manifest && !has_manifest {
        return Err(CrxError::MissingManifest);
    }

    Ok(ExtractionReport {
        files_extracted,
        dirs_created,
        total_bytes,
    })
}

// ---------------------------------------------------------------------------
// Protobuf varint parser (wire types 0, 1, 2, 5)
// ---------------------------------------------------------------------------

fn parse_protobuf_fields(input: &[u8]) -> Result<Vec<(u64, &[u8])>, CrxError> {
    let mut fields = Vec::new();
    let mut offset = 0usize;

    while offset < input.len() {
        let key = read_varint(input, &mut offset)?;
        let field_number = key >> 3;
        let wire_type = key & 0x07;
        if field_number == 0 {
            return Err(CrxError::InvalidProtobuf(
                "número de campo protobuf 0 inválido".into(),
            ));
        }

        match wire_type {
            0 => {
                let _ = read_varint(input, &mut offset)?;
            }
            1 => {
                offset = offset
                    .checked_add(8)
                    .filter(|&end| end <= input.len())
                    .ok_or_else(|| CrxError::InvalidProtobuf("campo fixed64 truncado".into()))?;
            }
            2 => {
                let len_u64 = read_varint(input, &mut offset)?;
                let len = usize::try_from(len_u64).map_err(|_| {
                    CrxError::InvalidProtobuf("longitud de campo desborda usize".into())
                })?;
                let end = offset.checked_add(len).ok_or_else(|| {
                    CrxError::InvalidProtobuf(
                        "desbordamiento al avanzar campo length-delimited".into(),
                    )
                })?;
                if end > input.len() {
                    return Err(CrxError::InvalidProtobuf(
                        "campo length-delimited excede el tamaño del buffer".into(),
                    ));
                }
                fields.push((field_number, &input[offset..end]));
                offset = end;
            }
            5 => {
                offset = offset
                    .checked_add(4)
                    .filter(|&end| end <= input.len())
                    .ok_or_else(|| CrxError::InvalidProtobuf("campo fixed32 truncado".into()))?;
            }
            other => {
                return Err(CrxError::InvalidProtobuf(format!(
                    "wire type protobuf no soportado: {other}"
                )));
            }
        }
    }

    Ok(fields)
}

fn parse_key_proof(input: &[u8]) -> Result<AsymmetricKeyProof, CrxError> {
    let fields = parse_protobuf_fields(input)?;
    let mut public_key: Option<Vec<u8>> = None;
    let mut signature: Option<Vec<u8>> = None;

    for (field_number, value) in fields {
        match field_number {
            1 => public_key = Some(value.to_vec()),
            2 => signature = Some(value.to_vec()),
            _ => {}
        }
    }

    Ok(AsymmetricKeyProof {
        public_key: public_key.ok_or_else(|| {
            CrxError::InvalidProtobuf("AsymmetricKeyProof sin public_key (campo 1)".into())
        })?,
        signature: signature.ok_or_else(|| {
            CrxError::InvalidProtobuf("AsymmetricKeyProof sin signature (campo 2)".into())
        })?,
    })
}

fn read_varint(input: &[u8], offset: &mut usize) -> Result<u64, CrxError> {
    let mut value = 0u64;
    let mut shift = 0u32;

    while *offset < input.len() {
        let byte = input[*offset];
        *offset += 1;

        if shift == 63 && (byte & 0xfe) != 0 {
            return Err(CrxError::InvalidProtobuf("varint excede 64 bits".into()));
        }
        value |= u64::from(byte & 0x7f) << shift;
        if (byte & 0x80) == 0 {
            return Ok(value);
        }
        shift += 7;
        if shift >= 64 {
            return Err(CrxError::InvalidProtobuf("varint demasiado largo".into()));
        }
    }

    Err(CrxError::InvalidProtobuf("varint truncado al final".into()))
}

// ---------------------------------------------------------------------------
// ASN.1 DER SubjectPublicKeyInfo unwrapper + ring signature verification
// ---------------------------------------------------------------------------

fn verify_rsa_spki_pkcs1_sha256(spki_der: &[u8], message: &[u8], sig: &[u8]) -> bool {
    let Some(rsa_pub_der) = unwrap_rsa_spki_der(spki_der) else {
        return false;
    };
    let key = UnparsedPublicKey::new(&signature::RSA_PKCS1_2048_8192_SHA256, rsa_pub_der);
    key.verify(message, sig).is_ok()
}

fn verify_ecdsa_p256_spki_sha256(spki_der: &[u8], message: &[u8], sig: &[u8]) -> bool {
    let Some(ec_point) = unwrap_ecdsa_p256_spki_der(spki_der) else {
        return false;
    };
    let key = UnparsedPublicKey::new(&signature::ECDSA_P256_SHA256_ASN1, ec_point);
    key.verify(message, sig).is_ok()
}

fn unwrap_rsa_spki_der(spki: &[u8]) -> Option<&[u8]> {
    let (outer, rest) = read_der_tlv(spki, 0x30)?;
    if !rest.is_empty() {
        return None;
    }
    let (alg_id, after_alg) = read_der_tlv(outer, 0x30)?;
    let (oid, alg_params) = read_der_tlv(alg_id, 0x06)?;
    if oid != OID_RSA_ENCRYPTION {
        return None;
    }
    // RFC 3279: los parámetros de rsaEncryption son NULL (05 00) o vacíos.
    if !alg_params.is_empty() && alg_params != [0x05, 0x00] {
        return None;
    }
    let (bit_string, trailing) = read_der_tlv(after_alg, 0x03)?;
    if !trailing.is_empty() || bit_string.len() < 2 || bit_string[0] != 0x00 {
        return None;
    }
    Some(&bit_string[1..])
}

fn unwrap_ecdsa_p256_spki_der(spki: &[u8]) -> Option<&[u8]> {
    let (outer, rest) = read_der_tlv(spki, 0x30)?;
    if !rest.is_empty() {
        return None;
    }
    let (alg_id, after_alg) = read_der_tlv(outer, 0x30)?;
    let (oid, alg_params) = read_der_tlv(alg_id, 0x06)?;
    if oid != OID_EC_PUBLIC_KEY {
        return None;
    }
    let (curve_oid, curve_rest) = read_der_tlv(alg_params, 0x06)?;
    if curve_oid != OID_PRIME256V1 || !curve_rest.is_empty() {
        return None;
    }
    let (bit_string, trailing) = read_der_tlv(after_alg, 0x03)?;
    if !trailing.is_empty() || bit_string.len() < 2 || bit_string[0] != 0x00 {
        return None;
    }
    Some(&bit_string[1..])
}

fn read_der_tlv(input: &[u8], expected_tag: u8) -> Option<(&[u8], &[u8])> {
    let (&tag, rest) = input.split_first()?;
    if tag != expected_tag {
        return None;
    }
    let (&len_byte, rest) = rest.split_first()?;
    let (len, after_len) = if (len_byte & 0x80) == 0 {
        (usize::from(len_byte), rest)
    } else {
        let count = usize::from(len_byte & 0x7f);
        if count == 0 || count > 4 || rest.len() < count {
            return None;
        }
        let mut len = 0usize;
        for &b in &rest[..count] {
            len = (len << 8) | usize::from(b);
        }
        // Rechazar longitudes no canónicas (forma larga para < 128 o ceros iniciales).
        if len < 128 || rest[0] == 0 {
            return None;
        }
        (len, &rest[count..])
    };
    if after_len.len() < len {
        return None;
    }
    Some((&after_len[..len], &after_len[len..]))
}

// ---------------------------------------------------------------------------
// Safe pure-Rust ZIP parser & extractor (Stored + Deflate + CRC-32)
// ---------------------------------------------------------------------------

struct ZipCentralEntry {
    name: String,
    compression_method: u16,
    crc32: u32,
    compressed_size: u64,
    uncompressed_size: u64,
    local_header_offset: usize,
    is_dir: bool,
}

fn parse_zip_central_directory(
    zip: &[u8],
    limits: ExtractLimits,
) -> Result<Vec<ZipCentralEntry>, CrxError> {
    if zip.len() < 22 {
        return Err(CrxError::InvalidZip(
            "archivo demasiado pequeño (< 22 bytes)".into(),
        ));
    }

    let search_start = zip.len().saturating_sub(22 + 65_535);
    let mut eocd_pos = None;
    for pos in (search_start..=zip.len() - 22).rev() {
        if &zip[pos..pos + 4] == b"PK\x05\x06" {
            let comment_len = read_le_u16(zip, pos + 20)? as usize;
            if pos + 22 + comment_len == zip.len() {
                eocd_pos = Some(pos);
                break;
            }
        }
    }
    let eocd = eocd_pos
        .ok_or_else(|| CrxError::InvalidZip("no se encontró End of Central Directory".into()))?;

    let disk_num = read_le_u16(zip, eocd + 4)?;
    let cd_disk = read_le_u16(zip, eocd + 6)?;
    if disk_num != 0 || cd_disk != 0 {
        return Err(CrxError::InvalidZip(
            "no se soportan archivos ZIP multi-volumen".into(),
        ));
    }

    let entry_count = read_le_u16(zip, eocd + 10)?;
    let cd_size = read_le_u32(zip, eocd + 12)?;
    let cd_offset = read_le_u32(zip, eocd + 16)?;
    if entry_count == u16::MAX || cd_size == u32::MAX || cd_offset == u32::MAX {
        return Err(CrxError::InvalidZip(
            "ZIP64 no está permitido para paquetes CRX3".into(),
        ));
    }

    let entry_count_usize = usize::from(entry_count);
    if entry_count_usize > limits.max_files {
        return Err(CrxError::SizeLimitExceeded(format!(
            "demasiadas entradas en ZIP: {entry_count_usize} > {}",
            limits.max_files
        )));
    }

    let cd_start = cd_offset as usize;
    let cd_end = cd_start
        .checked_add(cd_size as usize)
        .ok_or_else(|| CrxError::InvalidZip("desbordamiento en Central Directory".into()))?;
    if cd_end > eocd {
        return Err(CrxError::InvalidZip(
            "Central Directory fuera de los límites del ZIP".into(),
        ));
    }

    let mut entries = Vec::with_capacity(entry_count_usize);
    let mut cursor = cd_start;
    let mut declared_total = 0u64;

    for _ in 0..entry_count_usize {
        if cursor + 46 > cd_end {
            return Err(CrxError::InvalidZip(
                "entrada de Central Directory truncada".into(),
            ));
        }
        if &zip[cursor..cursor + 4] != b"PK\x01\x02" {
            return Err(CrxError::InvalidZip(
                "firma inválida en entrada de Central Directory".into(),
            ));
        }

        let version_made_by = read_le_u16(zip, cursor + 4)?;
        let flags = read_le_u16(zip, cursor + 8)?;
        if (flags & 0x0001) != 0 {
            return Err(CrxError::InvalidZip(
                "no se permiten entradas ZIP cifradas".into(),
            ));
        }

        let compression_method = read_le_u16(zip, cursor + 10)?;
        let crc32 = read_le_u32(zip, cursor + 16)?;
        let compressed_size = u64::from(read_le_u32(zip, cursor + 20)?);
        let uncompressed_size = u64::from(read_le_u32(zip, cursor + 24)?);
        let name_len = usize::from(read_le_u16(zip, cursor + 28)?);
        let extra_len = usize::from(read_le_u16(zip, cursor + 30)?);
        let comment_len = usize::from(read_le_u16(zip, cursor + 32)?);
        let ext_attr = read_le_u32(zip, cursor + 38)?;
        let local_header_offset = read_le_u32(zip, cursor + 42)? as usize;

        let name_start = cursor + 46;
        let name_end = name_start
            .checked_add(name_len)
            .ok_or_else(|| CrxError::InvalidZip("longitud de nombre inválida".into()))?;
        let next_cursor = name_end
            .checked_add(extra_len)
            .and_then(|v| v.checked_add(comment_len))
            .ok_or_else(|| CrxError::InvalidZip("desbordamiento en entrada ZIP".into()))?;
        if next_cursor > cd_end {
            return Err(CrxError::InvalidZip(
                "metadatos de entrada ZIP exceden Central Directory".into(),
            ));
        }

        let name = std::str::from_utf8(&zip[name_start..name_end])
            .map_err(|_| CrxError::InvalidZip("nombre de archivo ZIP no es UTF-8 válido".into()))?
            .to_owned();

        let unix_mode = (ext_attr >> 16) & 0o170000;
        let made_on_unix = (version_made_by >> 8) == 3;
        if unix_mode == 0o120000 {
            return Err(CrxError::SymlinkNotAllowed(name));
        }
        if (made_on_unix || unix_mode != 0)
            && unix_mode != 0
            && unix_mode != 0o100000
            && unix_mode != 0o040000
        {
            return Err(CrxError::UnsupportedEntryType(name));
        }

        let is_dir = name.ends_with('/') || unix_mode == 0o040000;
        if !is_dir {
            if uncompressed_size > limits.max_file_bytes {
                return Err(CrxError::SizeLimitExceeded(format!(
                    "'{}' declara {} bytes (> {})",
                    name, uncompressed_size, limits.max_file_bytes
                )));
            }
            declared_total = declared_total
                .checked_add(uncompressed_size)
                .ok_or_else(|| {
                    CrxError::SizeLimitExceeded("desbordamiento en tamaño total".into())
                })?;
            if declared_total > limits.max_total_bytes {
                return Err(CrxError::SizeLimitExceeded(format!(
                    "tamaño total declarado {declared_total} > {}",
                    limits.max_total_bytes
                )));
            }
            check_compression_ratio(&name, uncompressed_size, compressed_size, limits)?;
        }

        entries.push(ZipCentralEntry {
            name,
            compression_method,
            crc32,
            compressed_size,
            uncompressed_size,
            local_header_offset,
            is_dir,
        });
        cursor = next_cursor;
    }

    Ok(entries)
}

fn local_entry_compressed_slice<'a>(
    zip: &'a [u8],
    entry: &ZipCentralEntry,
) -> Result<&'a [u8], CrxError> {
    let pos = entry.local_header_offset;
    if pos + 30 > zip.len() || &zip[pos..pos + 4] != b"PK\x03\x04" {
        return Err(CrxError::InvalidZip(format!(
            "Local File Header inválido para '{}'",
            entry.name
        )));
    }
    let name_len = usize::from(read_le_u16(zip, pos + 26)?);
    let extra_len = usize::from(read_le_u16(zip, pos + 28)?);
    let data_start = pos
        .checked_add(30)
        .and_then(|v| v.checked_add(name_len))
        .and_then(|v| v.checked_add(extra_len))
        .ok_or_else(|| CrxError::InvalidZip("desbordamiento en Local File Header".into()))?;
    let comp_len = usize::try_from(entry.compressed_size)
        .map_err(|_| CrxError::SizeLimitExceeded("entrada comprimida demasiado grande".into()))?;
    let data_end = data_start
        .checked_add(comp_len)
        .ok_or_else(|| CrxError::InvalidZip("desbordamiento en datos comprimidos".into()))?;
    if data_end > zip.len() {
        return Err(CrxError::InvalidZip(format!(
            "datos truncados para '{}'",
            entry.name
        )));
    }
    Ok(&zip[data_start..data_end])
}

fn write_stored_entry(
    target: &Path,
    data: &[u8],
    entry: &ZipCentralEntry,
    cap: u64,
) -> Result<u64, CrxError> {
    let len = data.len() as u64;
    if len != entry.uncompressed_size {
        return Err(CrxError::InvalidZip(format!(
            "entrada Stored '{}' tiene tamaño inconsistente",
            entry.name
        )));
    }
    if len > cap {
        return Err(CrxError::SizeLimitExceeded(format!(
            "'{}' supera el límite permitido ({len} > {cap})",
            entry.name
        )));
    }

    let mut hasher = crc32fast::Hasher::new();
    hasher.update(data);
    if hasher.finalize() != entry.crc32 {
        return Err(CrxError::CrcMismatch(entry.name.clone()));
    }

    let mut file = create_regular_file(target)?;
    file.write_all(data)?;
    Ok(len)
}

fn write_deflated_entry(
    target: &Path,
    comp_slice: &[u8],
    entry: &ZipCentralEntry,
    cap: u64,
    limits: ExtractLimits,
) -> Result<u64, CrxError> {
    let decoder = DeflateDecoder::new(comp_slice);
    let mut limited = decoder.take(cap.saturating_add(1));
    let mut file = create_regular_file(target)?;
    let mut hasher = crc32fast::Hasher::new();
    let mut buf = [0u8; 16 * 1024];
    let mut total_written = 0u64;

    loop {
        let n = limited.read(&mut buf).map_err(|err| {
            CrxError::InvalidZip(format!("fallo al descomprimir '{}': {err}", entry.name))
        })?;
        if n == 0 {
            break;
        }
        total_written = total_written
            .checked_add(n as u64)
            .ok_or_else(|| CrxError::SizeLimitExceeded("desbordamiento al descomprimir".into()))?;
        if total_written > cap {
            return Err(CrxError::SizeLimitExceeded(format!(
                "'{}' excedió el límite real al descomprimir (> {cap} bytes)",
                entry.name
            )));
        }
        check_compression_ratio(&entry.name, total_written, comp_slice.len() as u64, limits)?;
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n])?;
    }

    if total_written != entry.uncompressed_size {
        return Err(CrxError::InvalidZip(format!(
            "tamaño descomprimido real ({total_written}) != declarado ({}) en '{}'",
            entry.uncompressed_size, entry.name
        )));
    }
    if hasher.finalize() != entry.crc32 {
        return Err(CrxError::CrcMismatch(entry.name.clone()));
    }

    Ok(total_written)
}

fn check_compression_ratio(
    entry_name: &str,
    uncompressed: u64,
    compressed: u64,
    limits: ExtractLimits,
) -> Result<(), CrxError> {
    if uncompressed <= MIN_RATIO_CHECK_BYTES || limits.max_compression_ratio == 0 {
        return Ok(());
    }
    if compressed == 0 {
        return Err(CrxError::ZipBomb {
            entry: entry_name.to_owned(),
            ratio: u64::MAX,
            max_ratio: limits.max_compression_ratio,
        });
    }
    let ratio = uncompressed / compressed;
    if ratio > limits.max_compression_ratio {
        return Err(CrxError::ZipBomb {
            entry: entry_name.to_owned(),
            ratio,
            max_ratio: limits.max_compression_ratio,
        });
    }
    Ok(())
}

fn validate_zip_entry_path(raw: &str) -> Result<PathBuf, CrxError> {
    if raw.is_empty() || raw.len() > 4096 {
        return Err(CrxError::UnsafePath(raw.to_owned()));
    }
    if raw.contains('\0') || raw.contains('\\') || raw.starts_with('/') {
        return Err(CrxError::UnsafePath(raw.to_owned()));
    }
    if raw.len() >= 2 && raw.as_bytes()[1] == b':' {
        return Err(CrxError::UnsafePath(raw.to_owned()));
    }

    let trimmed = raw.trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(CrxError::UnsafePath(raw.to_owned()));
    }

    let mut clean = PathBuf::new();
    for component in Path::new(trimmed).components() {
        match component {
            Component::Normal(part) => {
                if part.is_empty() || part == "." || part == ".." {
                    return Err(CrxError::UnsafePath(raw.to_owned()));
                }
                clean.push(part);
            }
            Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_)
            | Component::CurDir => {
                return Err(CrxError::UnsafePath(raw.to_owned()));
            }
        }
    }

    if clean.as_os_str().is_empty() {
        return Err(CrxError::UnsafePath(raw.to_owned()));
    }
    Ok(clean)
}

fn create_regular_file(path: &Path) -> Result<File, CrxError> {
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o644);
    }
    Ok(opts.open(path)?)
}

fn ensure_no_symlinks_on_disk(dir: &Path) -> Result<(), CrxError> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() {
            return Err(CrxError::SymlinkNotAllowed(path.display().to_string()));
        }
        if meta.is_dir() {
            ensure_no_symlinks_on_disk(&path)?;
        }
    }
    Ok(())
}

fn read_le_u16(bytes: &[u8], offset: usize) -> Result<u16, CrxError> {
    let slice = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| CrxError::InvalidZip("lectura u16 fuera de rango".into()))?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_le_u32(bytes: &[u8], offset: usize) -> Result<u32, CrxError> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| CrxError::InvalidCrxFormat("lectura u32 fuera de rango".into()))?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::DeflateEncoder;
    use flate2::Compression;
    use ring::rand::SystemRandom;
    use ring::rsa::KeyPair as RsaKeyPair;
    use ring::signature::{EcdsaKeyPair, KeyPair as _, ECDSA_P256_SHA256_ASN1_SIGNING};
    use std::process::Command;
    use std::sync::OnceLock;

    struct ZipFixtureEntry<'a> {
        name: &'a str,
        data: &'a [u8],
        compress: bool,
        unix_mode: u32,
        corrupt_crc: bool,
    }

    fn build_zip(entries: &[ZipFixtureEntry<'_>]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();

        for entry in entries {
            let local_offset = out.len() as u32;
            let mut hasher = crc32fast::Hasher::new();
            hasher.update(entry.data);
            let mut crc = hasher.finalize();
            if entry.corrupt_crc {
                crc ^= 0xdead_beef;
            }

            let (method, payload) = if entry.compress {
                let mut enc = DeflateEncoder::new(Vec::new(), Compression::best());
                enc.write_all(entry.data).expect("deflate write");
                (8u16, enc.finish().expect("deflate finish"))
            } else {
                (0u16, entry.data.to_vec())
            };

            let name_bytes = entry.name.as_bytes();
            // Local file header
            out.extend_from_slice(b"PK\x03\x04");
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            out.extend_from_slice(&(entry.data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name_bytes);
            out.extend_from_slice(&payload);

            // Central directory header
            let version_made_by: u16 = (3u16 << 8) | 20u16; // Unix
            let ext_attr: u32 = entry.unix_mode << 16;
            central.extend_from_slice(b"PK\x01\x02");
            central.extend_from_slice(&version_made_by.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&method.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            central.extend_from_slice(&(entry.data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&ext_attr.to_le_bytes());
            central.extend_from_slice(&local_offset.to_le_bytes());
            central.extend_from_slice(name_bytes);
        }

        let cd_offset = out.len() as u32;
        let cd_size = central.len() as u32;
        out.extend_from_slice(&central);

        // End of central directory
        out.extend_from_slice(b"PK\x05\x06");
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    fn encode_varint(mut v: u64, out: &mut Vec<u8>) {
        loop {
            let mut b = (v & 0x7f) as u8;
            v >>= 7;
            if v != 0 {
                b |= 0x80;
            }
            out.push(b);
            if v == 0 {
                break;
            }
        }
    }

    fn encode_pb_bytes(field_number: u64, data: &[u8], out: &mut Vec<u8>) {
        encode_varint((field_number << 3) | 2, out);
        encode_varint(data.len() as u64, out);
        out.extend_from_slice(data);
    }

    fn der_tlv(tag: u8, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(payload.len() + 6);
        out.push(tag);
        if payload.len() < 128 {
            out.push(payload.len() as u8);
        } else if payload.len() <= 0xff {
            out.push(0x81);
            out.push(payload.len() as u8);
        } else {
            out.push(0x82);
            out.push((payload.len() >> 8) as u8);
            out.push((payload.len() & 0xff) as u8);
        }
        out.extend_from_slice(payload);
        out
    }

    fn wrap_rsa_spki(rsa_pub_der: &[u8]) -> Vec<u8> {
        let mut alg_inner = der_tlv(0x06, OID_RSA_ENCRYPTION);
        alg_inner.extend_from_slice(&[0x05, 0x00]); // NULL
        let alg = der_tlv(0x30, &alg_inner);
        let mut bit_str = Vec::with_capacity(rsa_pub_der.len() + 1);
        bit_str.push(0x00);
        bit_str.extend_from_slice(rsa_pub_der);
        let mut outer = alg;
        outer.extend_from_slice(&der_tlv(0x03, &bit_str));
        der_tlv(0x30, &outer)
    }

    fn wrap_ecdsa_p256_spki(ec_point: &[u8]) -> Vec<u8> {
        let mut alg_inner = der_tlv(0x06, OID_EC_PUBLIC_KEY);
        alg_inner.extend_from_slice(&der_tlv(0x06, OID_PRIME256V1));
        let alg = der_tlv(0x30, &alg_inner);
        let mut bit_str = Vec::with_capacity(ec_point.len() + 1);
        bit_str.push(0x00);
        bit_str.extend_from_slice(ec_point);
        let mut outer = alg;
        outer.extend_from_slice(&der_tlv(0x03, &bit_str));
        der_tlv(0x30, &outer)
    }

    fn test_rsa_pkcs8() -> &'static Vec<u8> {
        static KEY: OnceLock<Vec<u8>> = OnceLock::new();
        KEY.get_or_init(|| {
            use std::process::Stdio;

            let gen = Command::new("openssl")
                .args([
                    "genpkey",
                    "-algorithm",
                    "RSA",
                    "-pkeyopt",
                    "rsa_keygen_bits:2048",
                    "-pkeyopt",
                    "rsa_keygen_pubexp:65537",
                ])
                .output()
                .expect("openssl genpkey debe existir en el SDK de pruebas");
            assert!(
                gen.status.success(),
                "openssl genpkey falló: {}",
                String::from_utf8_lossy(&gen.stderr)
            );

            let mut pkcs8 = Command::new("openssl")
                .args(["pkcs8", "-topk8", "-nocrypt", "-outform", "DER"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("openssl pkcs8");
            if let Some(mut stdin) = pkcs8.stdin.take() {
                stdin.write_all(&gen.stdout).expect("escribir PEM en pkcs8");
            }
            let output = pkcs8.wait_with_output().expect("esperar openssl pkcs8");
            assert!(
                output.status.success(),
                "openssl pkcs8 falló: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            output.stdout
        })
    }

    fn build_signed_rsa_crx3(
        zip_payload: &[u8],
        crx_id_override: Option<[u8; 16]>,
    ) -> (Vec<u8>, String) {
        let keypair = RsaKeyPair::from_pkcs8(test_rsa_pkcs8()).expect("parse RSA PKCS#8");
        let spki = wrap_rsa_spki(keypair.public().as_ref());
        let id = extension_id_from_public_key(&spki);

        let hash = digest(&SHA256, &spki);
        let crx_id_bytes = crx_id_override.unwrap_or_else(|| {
            let mut arr = [0u8; 16];
            arr.copy_from_slice(&hash.as_ref()[..16]);
            arr
        });

        let mut signed_header_data = Vec::new();
        encode_pb_bytes(1, &crx_id_bytes, &mut signed_header_data);

        let mut message = Vec::new();
        message.extend_from_slice(CRX_SIGNED_DATA_PREFIX);
        message.extend_from_slice(&(signed_header_data.len() as u32).to_le_bytes());
        message.extend_from_slice(&signed_header_data);
        message.extend_from_slice(zip_payload);

        let rng = SystemRandom::new();
        let mut signature_bytes = vec![0u8; keypair.public().modulus_len()];
        keypair
            .sign(
                &signature::RSA_PKCS1_SHA256,
                &rng,
                &message,
                &mut signature_bytes,
            )
            .expect("firma RSA");

        let mut proof_pb = Vec::new();
        encode_pb_bytes(1, &spki, &mut proof_pb);
        encode_pb_bytes(2, &signature_bytes, &mut proof_pb);

        let mut header_pb = Vec::new();
        encode_pb_bytes(2, &proof_pb, &mut header_pb);
        encode_pb_bytes(10000, &signed_header_data, &mut header_pb);

        let mut crx = Vec::new();
        crx.extend_from_slice(CRX_MAGIC);
        crx.extend_from_slice(&3u32.to_le_bytes());
        crx.extend_from_slice(&(header_pb.len() as u32).to_le_bytes());
        crx.extend_from_slice(&header_pb);
        crx.extend_from_slice(zip_payload);
        (crx, id)
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let seq = STAGING_SEQ.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "search-crx-test-{label}-{}-{seq}",
            std::process::id()
        ))
    }

    #[test]
    fn extension_id_and_cws_url_parsing() {
        let id = "cjpalhdlnbpafiamejdnhcphjbkeiagm";
        assert!(is_valid_extension_id(id));
        assert!(!is_valid_extension_id("cjpalhdlnbpafiamejdnhcphjbkeiagz"));
        assert!(!is_valid_extension_id("short"));

        let new_store = Url::parse(&format!(
            "https://chromewebstore.google.com/detail/ublock-origin/{id}?hl=es"
        ))
        .expect("url");
        assert_eq!(store_id_from_url(&new_store).as_deref(), Some(id));

        let old_store = Url::parse(&format!(
            "https://chrome.google.com/webstore/detail/ublock-origin/{id}"
        ))
        .expect("url");
        assert_eq!(store_id_from_url(&old_store).as_deref(), Some(id));

        let http_store = Url::parse(&format!(
            "http://chromewebstore.google.com/detail/ublock/{id}"
        ))
        .expect("url");
        assert_eq!(store_id_from_url(&http_store), None);

        let phishing =
            Url::parse(&format!("https://evil.example/detail/ublock/{id}")).expect("url");
        assert_eq!(store_id_from_url(&phishing), None);

        assert_eq!(
            extract_extension_id(&format!(
                "Install https://chromewebstore.google.com/detail/{id}/reviews"
            ))
            .as_deref(),
            Some(id)
        );

        let dl = cws_download_url(id).expect("download url");
        assert_eq!(dl.scheme(), "https");
        assert_eq!(dl.host_str(), Some("clients2.google.com"));
        assert!(dl.as_str().contains("acceptformat=crx3"));
        assert!(dl
            .as_str()
            .contains(&format!("prodversion={CHROME_VERSION}")));
        assert!(dl.as_str().contains(&format!("id%3D{id}")));
        assert!(cws_download_url("invalid").is_err());
    }

    #[test]
    fn verify_and_extract_valid_rsa_crx3() {
        let zip = build_zip(&[
            ZipFixtureEntry {
                name: "manifest.json",
                data: br#"{"manifest_version":3,"name":"Test","version":"1.0"}"#,
                compress: true,
                unix_mode: 0o100644,
                corrupt_crc: false,
            },
            ZipFixtureEntry {
                name: "scripts/content.js",
                data: b"console.log('ok');",
                compress: false,
                unix_mode: 0o100644,
                corrupt_crc: false,
            },
        ]);

        let (crx, id) = build_signed_rsa_crx3(&zip, None);
        let verified_zip = verify_crx3(&crx, &id).expect("firma RSA CRX3 válida");
        assert_eq!(verified_zip, zip.as_slice());

        let root = unique_temp_dir("valid-rsa");
        let dest = root.join(&id);
        let report = verify_and_extract_crx3(&crx, &id, &dest, ExtractLimits::default())
            .expect("extracción válida");
        assert_eq!(report.files_extracted, 2);
        assert!(dest.join("manifest.json").is_file());
        assert_eq!(
            fs::read_to_string(dest.join("scripts/content.js")).expect("read js"),
            "console.log('ok');"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn verify_ecdsa_p256_crx3() {
        let rng = SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng)
            .expect("generate p256");
        let keypair =
            EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8.as_ref(), &rng)
                .expect("load p256");
        let spki = wrap_ecdsa_p256_spki(keypair.public_key().as_ref());
        let id = extension_id_from_public_key(&spki);

        let zip = build_zip(&[ZipFixtureEntry {
            name: "manifest.json",
            data: br#"{"manifest_version":3,"name":"ECDSA","version":"1.0"}"#,
            compress: true,
            unix_mode: 0o100644,
            corrupt_crc: false,
        }]);

        let hash = digest(&SHA256, &spki);
        let mut signed_header_data = Vec::new();
        encode_pb_bytes(1, &hash.as_ref()[..16], &mut signed_header_data);

        let mut message = Vec::new();
        message.extend_from_slice(CRX_SIGNED_DATA_PREFIX);
        message.extend_from_slice(&(signed_header_data.len() as u32).to_le_bytes());
        message.extend_from_slice(&signed_header_data);
        message.extend_from_slice(&zip);

        let sig = keypair.sign(&rng, &message).expect("sign ecdsa");
        let mut proof_pb = Vec::new();
        encode_pb_bytes(1, &spki, &mut proof_pb);
        encode_pb_bytes(2, sig.as_ref(), &mut proof_pb);

        let mut header_pb = Vec::new();
        encode_pb_bytes(3, &proof_pb, &mut header_pb);
        encode_pb_bytes(10000, &signed_header_data, &mut header_pb);

        let mut crx = Vec::new();
        crx.extend_from_slice(CRX_MAGIC);
        crx.extend_from_slice(&3u32.to_le_bytes());
        crx.extend_from_slice(&(header_pb.len() as u32).to_le_bytes());
        crx.extend_from_slice(&header_pb);
        crx.extend_from_slice(&zip);

        let verified = verify_crx3(&crx, &id).expect("firma ECDSA válida");
        assert_eq!(verified, zip.as_slice());
    }

    #[test]
    fn rejects_tampered_zip_and_mismatched_id() {
        let zip = build_zip(&[ZipFixtureEntry {
            name: "manifest.json",
            data: br#"{"manifest_version":3}"#,
            compress: false,
            unix_mode: 0o100644,
            corrupt_crc: false,
        }]);
        let (mut crx, id) = build_signed_rsa_crx3(&zip, None);

        // ID equivocado -> rechazo inmediato
        let wrong_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        assert!(matches!(
            verify_crx3(&crx, wrong_id),
            Err(CrxError::SignatureVerificationFailed(_))
        ));

        // Alterar un byte del payload ZIP -> firma RSA inválida
        if let Some(last) = crx.last_mut() {
            *last ^= 0x01;
        }
        assert!(matches!(
            verify_crx3(&crx, &id),
            Err(CrxError::SignatureVerificationFailed(_))
        ));
    }

    #[test]
    fn rejects_path_traversal_symlinks_and_zip_bombs() {
        let root = unique_temp_dir("security");

        // 1. Path traversal (`../escape.txt`)
        let traversal_zip = build_zip(&[
            ZipFixtureEntry {
                name: "manifest.json",
                data: b"{}",
                compress: false,
                unix_mode: 0o100644,
                corrupt_crc: false,
            },
            ZipFixtureEntry {
                name: "../escape.txt",
                data: b"pwn",
                compress: false,
                unix_mode: 0o100644,
                corrupt_crc: false,
            },
        ]);
        let err = extract_zip_into(&traversal_zip, &root.join("t1"), ExtractLimits::default());
        assert!(matches!(err, Err(CrxError::UnsafePath(_))));
        assert!(!root.join("escape.txt").exists());

        // 2. Ruta absoluta (`/etc/passwd`)
        let abs_zip = build_zip(&[ZipFixtureEntry {
            name: "/etc/passwd",
            data: b"x",
            compress: false,
            unix_mode: 0o100644,
            corrupt_crc: false,
        }]);
        assert!(matches!(
            extract_zip_into(&abs_zip, &root.join("t2"), ExtractLimits::default()),
            Err(CrxError::UnsafePath(_))
        ));

        // 3. Enlace simbólico Unix (`S_IFLNK = 0o120000`)
        let symlink_zip = build_zip(&[
            ZipFixtureEntry {
                name: "manifest.json",
                data: b"{}",
                compress: false,
                unix_mode: 0o100644,
                corrupt_crc: false,
            },
            ZipFixtureEntry {
                name: "link",
                data: b"/etc/hosts",
                compress: false,
                unix_mode: 0o120777,
                corrupt_crc: false,
            },
        ]);
        assert!(matches!(
            extract_zip_into(&symlink_zip, &root.join("t3"), ExtractLimits::default()),
            Err(CrxError::SymlinkNotAllowed(_))
        ));

        // 4. Zip bomb (256 KiB de ceros comprimidos en ~250 bytes -> ratio > 500x)
        let zeros = vec![0u8; 256 * 1024];
        let bomb_zip = build_zip(&[
            ZipFixtureEntry {
                name: "manifest.json",
                data: b"{}",
                compress: false,
                unix_mode: 0o100644,
                corrupt_crc: false,
            },
            ZipFixtureEntry {
                name: "bomb.bin",
                data: &zeros,
                compress: true,
                unix_mode: 0o100644,
                corrupt_crc: false,
            },
        ]);
        assert!(matches!(
            extract_zip_into(&bomb_zip, &root.join("t4"), ExtractLimits::default()),
            Err(CrxError::ZipBomb { .. })
        ));

        // 5. Límite de tamaño explícito excedido
        let big_zip = build_zip(&[ZipFixtureEntry {
            name: "manifest.json",
            data: &[b'a'; 2048],
            compress: false,
            unix_mode: 0o100644,
            corrupt_crc: false,
        }]);
        let tight = ExtractLimits {
            max_file_bytes: 512,
            ..ExtractLimits::default()
        };
        assert!(matches!(
            extract_zip_into(&big_zip, &root.join("t5"), tight),
            Err(CrxError::SizeLimitExceeded(_))
        ));

        // 6. CRC corrupto
        let bad_crc_zip = build_zip(&[ZipFixtureEntry {
            name: "manifest.json",
            data: b"{}",
            compress: false,
            unix_mode: 0o100644,
            corrupt_crc: true,
        }]);
        assert!(matches!(
            extract_zip_into(&bad_crc_zip, &root.join("t6"), ExtractLimits::default()),
            Err(CrxError::CrcMismatch(_))
        ));

        let _ = fs::remove_dir_all(&root);
    }
}
