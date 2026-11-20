//! Comprobador y verificador criptográfico de actualizaciones firmadas (Fase 8).
//!
//! - Se desactiva de forma incondicional dentro de Flatpak (`/.flatpak-info`),
//!   donde las actualizaciones corresponden a Flathub / el gestor Flatpak.
//! - En instalaciones independientes (tarball / AppImage), verifica la firma
//!   Ed25519 del manifiesto JSON y el hash SHA-256 de los artefactos con `ring`
//!   antes de notificar al usuario. Nunca instala sin confirmación explícita.

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use ring::digest::{Context as Sha256Context, SHA256};
use ring::signature::{self, UnparsedPublicKey};
pub use semver::Version;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use thiserror::Error;
use url::Url;

/// Ruta estándar que crea `flatpak run` / `bwrap` dentro del sandbox Flatpak.
pub const FLATPAK_INFO_PATH: &str = "/.flatpak-info";

/// Intervalo predeterminado entre comprobaciones automáticas (24 horas).
pub const DEFAULT_CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;

/// Tamaño máximo aceptado para el manifiesto de lanzamiento (256 KiB).
pub const MAX_MANIFEST_BYTES: usize = 256 * 1024;

/// Hosts HTTPS de confianza por defecto para descargas de lanzamientos.
pub const DEFAULT_ALLOWED_HOSTS: &[&str] = &["github.com", "objects.githubusercontent.com"];

/// Errores del subsistema de actualizaciones.
#[derive(Debug, Error)]
pub enum UpdaterError {
    #[error("el actualizador está desactivado en Flatpak")]
    DisabledInFlatpak,
    #[error("manifiesto demasiado grande ({0} bytes > {MAX_MANIFEST_BYTES})")]
    ManifestTooLarge(usize),
    #[error("clave pública Ed25519 inválida: {0}")]
    InvalidPublicKey(String),
    #[error("firma Ed25519 inválida o mal codificada: {0}")]
    InvalidSignature(String),
    #[error("fallo de verificación de firma Ed25519 sobre el manifiesto")]
    SignatureMismatch,
    #[error("manifiesto JSON inválido: {0}")]
    InvalidManifestJson(#[from] serde_json::Error),
    #[error("versión SemVer inválida '{version}': {reason}")]
    InvalidVersion { version: String, reason: String },
    #[error("URL de actualización rechazada ('{url}'): {reason}")]
    InvalidUrl { url: String, reason: String },
    #[error("hash SHA-256 inválido o no coincide (esperado '{expected}', calculado '{actual}')")]
    Sha256Mismatch { expected: String, actual: String },
    #[error("formato de hash SHA-256 inválido: {0}")]
    InvalidSha256Format(String),
    #[error("tamaño de artefacto fuera de límite ({actual} bytes > {max} bytes)")]
    ArtifactTooLarge { actual: u64, max: u64 },
    #[error("error de E/S: {0}")]
    Io(#[from] io::Error),
}

/// Formato de distribución de un artefacto de lanzamiento en Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReleaseFormat {
    Tarball,
    AppImage,
    Flatpak,
    Deb,
    Rpm,
}

/// Artefacto descargable listado en un manifiesto de lanzamiento firmado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseAsset {
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub arch: String,
    pub format: ReleaseFormat,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// Manifiesto JSON firmado que publica cada nueva versión de Search para Linux.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseManifest {
    pub version: String,
    pub build: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default)]
    pub assets: Vec<ReleaseAsset>,
}

/// Manifiesto ya validado criptográficamente y con SemVer parseado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedRelease {
    pub version: Version,
    pub build: u64,
    pub published_at: Option<String>,
    pub notes: Option<String>,
    pub assets: Vec<ReleaseAsset>,
}

/// Configuración de política para evaluar una comprobación de actualizaciones.
#[derive(Debug, Clone)]
pub struct UpdatePolicy {
    pub current_version: Version,
    pub current_build: u64,
    pub check_enabled: bool,
    pub flatpak_info_path: PathBuf,
    pub allowed_hosts: Vec<String>,
    pub allow_insecure_http_for_tests: bool,
    pub check_interval_secs: u64,
}

impl UpdatePolicy {
    /// Crea la política predeterminada para una versión y build locales,
    /// restringiendo las descargas a `DEFAULT_ALLOWED_HOSTS` (denegar por defecto).
    pub fn new(current_version: Version, current_build: u64, check_enabled: bool) -> Self {
        Self {
            current_version,
            current_build,
            check_enabled,
            flatpak_info_path: PathBuf::from(FLATPAK_INFO_PATH),
            allowed_hosts: DEFAULT_ALLOWED_HOSTS
                .iter()
                .map(|h| (*h).to_owned())
                .collect(),
            allow_insecure_http_for_tests: false,
            check_interval_secs: DEFAULT_CHECK_INTERVAL_SECS,
        }
    }

    /// Devuelve `true` si el proceso se ejecuta dentro de un contenedor Flatpak.
    pub fn is_flatpak(&self) -> bool {
        is_flatpak_at(&self.flatpak_info_path)
    }

    /// Indica si corresponde ejecutar una comprobación remota en este instante.
    pub fn should_check(&self, last_checked_unix: Option<u64>, now_unix: u64) -> bool {
        if self.is_flatpak() || !self.check_enabled {
            return false;
        }
        is_check_due(last_checked_unix, now_unix, self.check_interval_secs)
    }
}

/// Resultado de evaluar un manifiesto firmado frente a la política local.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCheckOutcome {
    /// En Flatpak las actualizaciones se delegan a Flathub / `flatpak update`.
    DisabledInFlatpak,
    /// El usuario desactivó `update.check` en ajustes.
    DisabledByUser,
    /// La versión instalada ya es igual o más reciente que la del manifiesto.
    UpToDate { current: Version, latest: Version },
    /// Existe una versión más nueva verificada criptográficamente; requiere
    /// confirmación explícita del usuario antes de descargar o instalar.
    Available(VerifiedRelease),
}

/// Detecta si el proceso actual corre dentro de Flatpak comprobando `/.flatpak-info`.
pub fn is_flatpak() -> bool {
    is_flatpak_at(Path::new(FLATPAK_INFO_PATH))
}

/// Variante parametrizable para tests unitarios de la detección de Flatpak.
pub fn is_flatpak_at(marker_path: &Path) -> bool {
    marker_path.exists()
}

/// Indica si ha transcurrido al menos `interval_secs` desde `last_checked_unix`.
pub fn is_check_due(last_checked_unix: Option<u64>, now_unix: u64, interval_secs: u64) -> bool {
    match last_checked_unix {
        None => true,
        Some(last) => now_unix.saturating_sub(last) >= interval_secs,
    }
}

/// Calcula el digest SHA-256 en hexadecimal minúsculas de un buffer en memoria.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut ctx = Sha256Context::new(&SHA256);
    ctx.update(data);
    bytes_to_hex(ctx.finish().as_ref())
}

/// Calcula el digest SHA-256 en hexadecimal minúsculas leyendo un archivo en streaming.
pub fn sha256_file(path: &Path) -> Result<String, UpdaterError> {
    let mut file = File::open(path)?;
    let mut ctx = Sha256Context::new(&SHA256);
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        ctx.update(&buf[..n]);
    }
    Ok(bytes_to_hex(ctx.finish().as_ref()))
}

/// Verifica que `data` coincida con el digest SHA-256 hexadecimal esperado.
pub fn verify_sha256(data: &[u8], expected_hex: &str) -> Result<(), UpdaterError> {
    let normalized = normalize_sha256_hex(expected_hex)?;
    let actual = sha256_hex(data);
    if !constant_time_eq(actual.as_bytes(), normalized.as_bytes()) {
        return Err(UpdaterError::Sha256Mismatch {
            expected: normalized,
            actual,
        });
    }
    Ok(())
}

/// Verifica el tamaño máximo y el digest SHA-256 de un archivo descargado.
pub fn verify_file_sha256(
    path: &Path,
    expected_hex: &str,
    max_bytes: u64,
) -> Result<(), UpdaterError> {
    let normalized = normalize_sha256_hex(expected_hex)?;
    let meta = std::fs::metadata(path)?;
    if meta.len() > max_bytes {
        return Err(UpdaterError::ArtifactTooLarge {
            actual: meta.len(),
            max: max_bytes,
        });
    }
    let actual = sha256_file(path)?;
    if !constant_time_eq(actual.as_bytes(), normalized.as_bytes()) {
        return Err(UpdaterError::Sha256Mismatch {
            expected: normalized,
            actual,
        });
    }
    Ok(())
}

/// Verifica una firma Ed25519 (64 bytes) sobre `payload` usando una clave pública Ed25519 (32 bytes).
pub fn verify_ed25519_bytes(
    public_key: &[u8],
    payload: &[u8],
    signature: &[u8],
) -> Result<(), UpdaterError> {
    if public_key.len() != 32 {
        return Err(UpdaterError::InvalidPublicKey(format!(
            "se esperaban 32 bytes, se recibieron {}",
            public_key.len()
        )));
    }
    if signature.len() != 64 {
        return Err(UpdaterError::InvalidSignature(format!(
            "se esperaban 64 bytes, se recibieron {}",
            signature.len()
        )));
    }
    let verifier = UnparsedPublicKey::new(&signature::ED25519, public_key);
    verifier
        .verify(payload, signature)
        .map_err(|_| UpdaterError::SignatureMismatch)
}

/// Verifica una firma Ed25519 codificada en hexadecimal o Base64 estándar.
pub fn verify_ed25519_encoded(
    public_key_encoded: &str,
    payload: &[u8],
    signature_encoded: &str,
) -> Result<(), UpdaterError> {
    let pk =
        decode_fixed_key_or_sig(public_key_encoded, 32).map_err(UpdaterError::InvalidPublicKey)?;
    let sig =
        decode_fixed_key_or_sig(signature_encoded, 64).map_err(UpdaterError::InvalidSignature)?;
    verify_ed25519_bytes(&pk, payload, &sig)
}

/// Verifica firma Ed25519, esquema JSON, URLs HTTPS y compara versión/build frente a `policy`.
pub fn evaluate_signed_manifest(
    policy: &UpdatePolicy,
    public_key_encoded: &str,
    manifest_bytes: &[u8],
    signature_encoded: &str,
) -> Result<UpdateCheckOutcome, UpdaterError> {
    if policy.is_flatpak() {
        return Ok(UpdateCheckOutcome::DisabledInFlatpak);
    }
    if !policy.check_enabled {
        return Ok(UpdateCheckOutcome::DisabledByUser);
    }
    if manifest_bytes.len() > MAX_MANIFEST_BYTES {
        return Err(UpdaterError::ManifestTooLarge(manifest_bytes.len()));
    }

    // Siempre verificar la firma criptográfica ANTES de parsear JSON no confiable.
    verify_ed25519_encoded(public_key_encoded, manifest_bytes, signature_encoded)?;

    let manifest: ReleaseManifest = serde_json::from_slice(manifest_bytes)?;
    let latest_version =
        Version::parse(&manifest.version).map_err(|err| UpdaterError::InvalidVersion {
            version: manifest.version.clone(),
            reason: err.to_string(),
        })?;

    for asset in &manifest.assets {
        validate_asset_url(&asset.url, policy)?;
        let _ = normalize_sha256_hex(&asset.sha256)?;
    }

    let is_newer = latest_version > policy.current_version
        || (latest_version == policy.current_version && manifest.build > policy.current_build);

    if !is_newer {
        return Ok(UpdateCheckOutcome::UpToDate {
            current: policy.current_version.clone(),
            latest: latest_version,
        });
    }

    Ok(UpdateCheckOutcome::Available(VerifiedRelease {
        version: latest_version,
        build: manifest.build,
        published_at: manifest.published_at,
        notes: manifest.notes,
        assets: manifest.assets,
    }))
}

/// Verifica un artefacto descargado (SHA-256 obligatorio y, si viene presente, firma Ed25519).
pub fn verify_downloaded_asset(
    asset: &ReleaseAsset,
    artifact_bytes: &[u8],
    public_key_encoded: Option<&str>,
) -> Result<(), UpdaterError> {
    if asset.size > 0 && (artifact_bytes.len() as u64) > asset.size {
        return Err(UpdaterError::ArtifactTooLarge {
            actual: artifact_bytes.len() as u64,
            max: asset.size,
        });
    }
    verify_sha256(artifact_bytes, &asset.sha256)?;
    if let (Some(pk), Some(sig)) = (public_key_encoded, asset.signature.as_deref()) {
        verify_ed25519_encoded(pk, artifact_bytes, sig)?;
    }
    Ok(())
}

fn validate_asset_url(raw: &str, policy: &UpdatePolicy) -> Result<(), UpdaterError> {
    let parsed = Url::parse(raw).map_err(|err| UpdaterError::InvalidUrl {
        url: raw.to_owned(),
        reason: err.to_string(),
    })?;
    let scheme_ok = parsed.scheme() == "https"
        || (policy.allow_insecure_http_for_tests && parsed.scheme() == "http");
    if !scheme_ok {
        return Err(UpdaterError::InvalidUrl {
            url: raw.to_owned(),
            reason: format!(
                "esquema '{}' no permitido (se requiere https)",
                parsed.scheme()
            ),
        });
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(UpdaterError::InvalidUrl {
            url: raw.to_owned(),
            reason: "no se permiten credenciales embebidas en la URL".into(),
        });
    }
    if policy.allowed_hosts.is_empty() {
        return Err(UpdaterError::InvalidUrl {
            url: raw.to_owned(),
            reason: "la lista de hosts permitidos está vacía (denegado por defecto)".into(),
        });
    }
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    let allowed = policy
        .allowed_hosts
        .iter()
        .any(|h| h.eq_ignore_ascii_case(&host));
    if !allowed {
        return Err(UpdaterError::InvalidUrl {
            url: raw.to_owned(),
            reason: format!("el host '{host}' no está en la lista de confianza"),
        });
    }
    Ok(())
}

fn normalize_sha256_hex(input: &str) -> Result<String, UpdaterError> {
    let trimmed = input.trim().to_ascii_lowercase();
    if trimmed.len() != 64 || !trimmed.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(UpdaterError::InvalidSha256Format(format!(
            "se esperaban 64 dígitos hexadecimales, se recibió longitud {}",
            trimmed.len()
        )));
    }
    Ok(trimmed)
}

fn decode_fixed_key_or_sig(encoded: &str, expected_len: usize) -> Result<Vec<u8>, String> {
    let trimmed = encoded.trim();
    if trimmed.len() == expected_len * 2 && trimmed.bytes().all(|b| b.is_ascii_hexdigit()) {
        return hex_to_bytes(trimmed);
    }
    let decoded = BASE64_STANDARD
        .decode(trimmed)
        .map_err(|err| format!("no es hex ni Base64 válido: {err}"))?;
    if decoded.len() != expected_len {
        return Err(format!(
            "longitud decodificada {} != esperada {expected_len}",
            decoded.len()
        ));
    }
    Ok(decoded)
}

fn hex_to_bytes(hex: &str) -> Result<Vec<u8>, String> {
    let bytes = hex.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return Err("longitud hexadecimal impar".into());
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.as_chunks::<2>().0 {
        let hi = from_hex_nibble(pair[0]).ok_or_else(|| "carácter hex inválido".to_owned())?;
        let lo = from_hex_nibble(pair[1]).ok_or_else(|| "carácter hex inválido".to_owned())?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn from_hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(char::from(HEX[usize::from(b >> 4)]));
        out.push(char::from(HEX[usize::from(b & 0x0f)]));
    }
    out
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair as _};
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_SEQ: AtomicU64 = AtomicU64::new(1);

    fn temp_path(name: &str) -> PathBuf {
        let seq = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "search-updater-test-{name}-{}-{seq}",
            std::process::id()
        ))
    }

    fn generate_ed25519_keypair() -> Ed25519KeyPair {
        let rng = SystemRandom::new();
        let doc = Ed25519KeyPair::generate_pkcs8(&rng).expect("generate ed25519");
        Ed25519KeyPair::from_pkcs8(doc.as_ref()).expect("load ed25519")
    }

    fn sample_manifest(version: &str, build: u64, url: &str, payload: &[u8]) -> Vec<u8> {
        let manifest = ReleaseManifest {
            version: version.to_owned(),
            build,
            published_at: Some("2026-11-18T12:00:00Z".into()),
            notes: Some("Mejoras de rendimiento y correcciones".into()),
            assets: vec![ReleaseAsset {
                url: url.to_owned(),
                sha256: sha256_hex(payload),
                size: payload.len() as u64,
                arch: "x86_64".into(),
                format: ReleaseFormat::Tarball,
                signature: None,
            }],
        };
        serde_json::to_vec(&manifest).expect("serialize manifest")
    }

    #[test]
    fn flatpak_marker_unconditionally_disables_updater() {
        let marker = temp_path("flatpak-info");
        fs::write(&marker, "[Application]\nname=dev.senes.Dot\n").expect("write marker");

        let mut policy = UpdatePolicy::new(Version::new(0, 1, 0), 1, true);
        policy.flatpak_info_path = marker.clone();
        assert!(policy.is_flatpak());
        assert!(!policy.should_check(None, 1_700_000_000));

        let outcome = evaluate_signed_manifest(&policy, "invalid", b"invalid", "invalid")
            .expect("debe devolver DisabledInFlatpak sin comprobar red ni firma");
        assert_eq!(outcome, UpdateCheckOutcome::DisabledInFlatpak);

        let _ = fs::remove_file(&marker);
    }

    #[test]
    fn signed_manifest_verification_and_version_comparison() {
        let keypair = generate_ed25519_keypair();
        let pub_hex = bytes_to_hex(keypair.public_key().as_ref());
        let pub_b64 = BASE64_STANDARD.encode(keypair.public_key().as_ref());

        let artifact = b"tarball-binary-payload-example";
        let manifest_bytes = sample_manifest(
            "0.2.0",
            20,
            "https://github.com/search-linux/search/releases/download/v0.2.0/search-0.2.0-x86_64.tar.gz",
            artifact,
        );
        let sig = keypair.sign(&manifest_bytes);
        let sig_b64 = BASE64_STANDARD.encode(sig.as_ref());
        let sig_hex = bytes_to_hex(sig.as_ref());

        let mut policy = UpdatePolicy::new(Version::new(0, 1, 0), 10, true);
        policy.flatpak_info_path = temp_path("no-flatpak");
        policy.allowed_hosts = vec!["github.com".into()];

        // Firma válida (hex + base64) -> disponible
        let outcome = evaluate_signed_manifest(&policy, &pub_hex, &manifest_bytes, &sig_b64)
            .expect("manifiesto válido");
        match outcome {
            UpdateCheckOutcome::Available(rel) => {
                assert_eq!(rel.version, Version::new(0, 2, 0));
                assert_eq!(rel.build, 20);
                verify_downloaded_asset(&rel.assets[0], artifact, Some(&pub_b64))
                    .expect("artefacto válido");
                assert!(matches!(
                    verify_downloaded_asset(&rel.assets[0], b"tampered", Some(&pub_b64)),
                    Err(UpdaterError::Sha256Mismatch { .. })
                ));
            }
            other => panic!("se esperaba Available, se obtuvo {other:?}"),
        }

        // Versión igual y mismo build -> UpToDate
        policy.current_version = Version::new(0, 2, 0);
        policy.current_build = 20;
        let up_to_date = evaluate_signed_manifest(&policy, &pub_b64, &manifest_bytes, &sig_hex)
            .expect("up to date");
        assert!(matches!(up_to_date, UpdateCheckOutcome::UpToDate { .. }));

        // Manifiesto manipulado -> SignatureMismatch
        let mut tampered = manifest_bytes.clone();
        tampered[0] ^= 0x01;
        assert!(matches!(
            evaluate_signed_manifest(&policy, &pub_hex, &tampered, &sig_b64),
            Err(UpdaterError::SignatureMismatch)
        ));

        // Clave pública de otro firmante -> SignatureMismatch
        let other = generate_ed25519_keypair();
        let other_pub = bytes_to_hex(other.public_key().as_ref());
        assert!(matches!(
            evaluate_signed_manifest(&policy, &other_pub, &manifest_bytes, &sig_b64),
            Err(UpdaterError::SignatureMismatch)
        ));
    }

    #[test]
    fn rejects_insecure_http_and_untrusted_host() {
        let keypair = generate_ed25519_keypair();
        let pub_hex = bytes_to_hex(keypair.public_key().as_ref());
        let artifact = b"payload";
        let manifest_bytes = sample_manifest(
            "0.2.0",
            2,
            "http://github.com/search-linux/search/releases/download/v0.2.0/search.tar.gz",
            artifact,
        );
        let sig_hex = bytes_to_hex(keypair.sign(&manifest_bytes).as_ref());

        let mut policy = UpdatePolicy::new(Version::new(0, 1, 0), 1, true);
        policy.flatpak_info_path = temp_path("no-flatpak-http");
        policy.allowed_hosts = vec!["github.com".into()];

        assert!(matches!(
            evaluate_signed_manifest(&policy, &pub_hex, &manifest_bytes, &sig_hex),
            Err(UpdaterError::InvalidUrl { .. })
        ));

        policy.allow_insecure_http_for_tests = true;
        assert!(matches!(
            evaluate_signed_manifest(&policy, &pub_hex, &manifest_bytes, &sig_hex),
            Ok(UpdateCheckOutcome::Available(_))
        ));

        // Lista de hosts permitidos vacía -> denegado por defecto incluso con HTTPS
        let https_manifest = sample_manifest(
            "0.2.0",
            2,
            "https://github.com/search-linux/search/releases/download/v0.2.0/search.tar.gz",
            artifact,
        );
        let https_sig = bytes_to_hex(keypair.sign(&https_manifest).as_ref());
        let mut empty_hosts_policy = UpdatePolicy::new(Version::new(0, 1, 0), 1, true);
        empty_hosts_policy.flatpak_info_path = temp_path("no-flatpak-empty-hosts");
        empty_hosts_policy.allowed_hosts.clear();
        assert!(matches!(
            evaluate_signed_manifest(&empty_hosts_policy, &pub_hex, &https_manifest, &https_sig),
            Err(UpdaterError::InvalidUrl { .. })
        ));
    }

    #[test]
    fn file_sha256_and_daily_schedule() {
        let file = temp_path("artifact.bin");
        fs::write(&file, b"search-linux-release-bytes").expect("write");
        let expected = sha256_hex(b"search-linux-release-bytes");
        verify_file_sha256(&file, &expected, 1024).expect("hash válido");
        assert!(matches!(
            verify_file_sha256(&file, &expected, 4),
            Err(UpdaterError::ArtifactTooLarge { .. })
        ));
        let _ = fs::remove_file(&file);

        assert!(is_check_due(None, 100, DEFAULT_CHECK_INTERVAL_SECS));
        assert!(!is_check_due(
            Some(1000),
            1000 + 3600,
            DEFAULT_CHECK_INTERVAL_SECS
        ));
        assert!(is_check_due(
            Some(1000),
            1000 + DEFAULT_CHECK_INTERVAL_SECS,
            DEFAULT_CHECK_INTERVAL_SECS
        ));
    }
}
