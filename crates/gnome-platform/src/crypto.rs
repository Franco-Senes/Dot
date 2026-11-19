//! Envoltorio mínimo y seguro sobre `libcrypto` (OpenSSL 3) para los importadores de Vault.
//!
//! Solo expone lo que necesitan los formatos de Firefox (NSS `key4.db` + `logins.json`) y
//! Chromium (`Login Data` `v10`/`v11`): AES-128/256-CBC, 3DES-EDE-CBC, PBKDF2-HMAC-SHA1/SHA256,
//! SHA-1/SHA-256, HMAC-SHA1 y bytes aleatorios. Toda salida que pueda contener material secreto
//! se devuelve en `Zeroizing<Vec<u8>>`, que se pone a cero al liberarse.

use std::fmt;

use libc::{c_int, c_uchar, c_uint, c_void, size_t};
use zeroize::Zeroizing;

#[repr(C)]
struct EvpCipherCtx {
    _private: [u8; 0],
}
#[repr(C)]
struct EvpCipher {
    _private: [u8; 0],
}
#[repr(C)]
struct EvpMd {
    _private: [u8; 0],
}

#[link(name = "crypto")]
extern "C" {
    fn EVP_CIPHER_CTX_new() -> *mut EvpCipherCtx;
    fn EVP_CIPHER_CTX_free(ctx: *mut EvpCipherCtx);
    fn EVP_CIPHER_CTX_set_padding(ctx: *mut EvpCipherCtx, pad: c_int) -> c_int;
    fn EVP_aes_128_cbc() -> *const EvpCipher;
    fn EVP_aes_256_cbc() -> *const EvpCipher;
    fn EVP_des_ede3_cbc() -> *const EvpCipher;
    fn EVP_CIPHER_get_key_length(cipher: *const EvpCipher) -> c_int;
    fn EVP_CIPHER_get_iv_length(cipher: *const EvpCipher) -> c_int;
    fn EVP_CIPHER_get_block_size(cipher: *const EvpCipher) -> c_int;
    fn EVP_CipherInit_ex(
        ctx: *mut EvpCipherCtx,
        cipher: *const EvpCipher,
        engine: *mut c_void,
        key: *const c_uchar,
        iv: *const c_uchar,
        enc: c_int,
    ) -> c_int;
    fn EVP_CipherUpdate(
        ctx: *mut EvpCipherCtx,
        out: *mut c_uchar,
        outl: *mut c_int,
        input: *const c_uchar,
        inl: c_int,
    ) -> c_int;
    fn EVP_CipherFinal_ex(ctx: *mut EvpCipherCtx, out: *mut c_uchar, outl: *mut c_int) -> c_int;
    fn EVP_sha1() -> *const EvpMd;
    fn EVP_sha256() -> *const EvpMd;
    fn EVP_Digest(
        data: *const c_void,
        count: size_t,
        md: *mut c_uchar,
        size: *mut c_uint,
        md_type: *const EvpMd,
        engine: *mut c_void,
    ) -> c_int;
    fn HMAC(
        md: *const EvpMd,
        key: *const c_void,
        key_len: c_int,
        data: *const c_uchar,
        data_len: size_t,
        out: *mut c_uchar,
        out_len: *mut c_uint,
    ) -> *mut c_uchar;
    fn PKCS5_PBKDF2_HMAC(
        pass: *const libc::c_char,
        passlen: c_int,
        salt: *const c_uchar,
        saltlen: c_int,
        iter: c_int,
        digest: *const EvpMd,
        keylen: c_int,
        out: *mut c_uchar,
    ) -> c_int;
    fn RAND_bytes(buf: *mut c_uchar, num: c_int) -> c_int;
}

/// Error de una operación criptográfica. Nunca incluye datos de entrada ni de salida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CryptoError {
    /// Longitud de clave o IV incorrecta para el cifrado pedido.
    BadKeyOrIv,
    /// Entrada vacía, demasiado grande o no múltiplo del bloque.
    BadInput,
    /// El relleno PKCS#7 no es válido (clave equivocada o datos corruptos).
    BadPadding,
    /// OpenSSL devolvió un error interno.
    Backend,
}

impl fmt::Display for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            CryptoError::BadKeyOrIv => "longitud de clave o IV no válida",
            CryptoError::BadInput => "datos cifrados con longitud no válida",
            CryptoError::BadPadding => "relleno no válido (clave incorrecta o datos corruptos)",
            CryptoError::Backend => "error interno de libcrypto",
        };
        f.write_str(text)
    }
}

impl std::error::Error for CryptoError {}

/// Cifrados en modo CBC con relleno PKCS#7 que usan los navegadores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cipher {
    /// Chromium `v10`/`v11` en Linux.
    Aes128Cbc,
    /// NSS moderno (Firefox ≥ 58 con `key4.db` PBES2 y logins `AES-256-CBC`).
    Aes256Cbc,
    /// NSS antiguo (`PBE-SHA1-3DES` y logins `DES-EDE3-CBC`).
    TripleDesCbc,
}

impl Cipher {
    fn raw(self) -> *const EvpCipher {
        unsafe {
            match self {
                Cipher::Aes128Cbc => EVP_aes_128_cbc(),
                Cipher::Aes256Cbc => EVP_aes_256_cbc(),
                Cipher::TripleDesCbc => EVP_des_ede3_cbc(),
            }
        }
    }

    /// Longitud de clave en bytes.
    pub fn key_len(self) -> usize {
        usize::try_from(unsafe { EVP_CIPHER_get_key_length(self.raw()) }).unwrap_or(0)
    }

    /// Longitud de IV en bytes (= tamaño de bloque en CBC).
    pub fn iv_len(self) -> usize {
        usize::try_from(unsafe { EVP_CIPHER_get_iv_length(self.raw()) }).unwrap_or(0)
    }

    /// Tamaño de bloque en bytes.
    pub fn block_size(self) -> usize {
        usize::try_from(unsafe { EVP_CIPHER_get_block_size(self.raw()) }).unwrap_or(0)
    }
}

struct Ctx(*mut EvpCipherCtx);

impl Drop for Ctx {
    fn drop(&mut self) {
        // `EVP_CIPHER_CTX_free` limpia el estado interno (incluida la clave expandida).
        unsafe { EVP_CIPHER_CTX_free(self.0) }
    }
}

fn run_cbc(
    cipher: Cipher,
    key: &[u8],
    iv: &[u8],
    input: &[u8],
    encrypt: bool,
) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    let block = cipher.block_size();
    if block == 0 || key.len() != cipher.key_len() || iv.len() != cipher.iv_len() {
        return Err(CryptoError::BadKeyOrIv);
    }
    if !encrypt && (input.is_empty() || !input.len().is_multiple_of(block)) {
        return Err(CryptoError::BadInput);
    }
    let in_len = c_int::try_from(input.len()).map_err(|_| CryptoError::BadInput)?;
    let ctx = Ctx(unsafe { EVP_CIPHER_CTX_new() });
    if ctx.0.is_null() {
        return Err(CryptoError::Backend);
    }
    let ok = unsafe {
        EVP_CipherInit_ex(
            ctx.0,
            cipher.raw(),
            std::ptr::null_mut(),
            key.as_ptr(),
            iv.as_ptr(),
            c_int::from(encrypt),
        )
    };
    if ok != 1 || unsafe { EVP_CIPHER_CTX_set_padding(ctx.0, 1) } != 1 {
        return Err(CryptoError::Backend);
    }
    let mut out = Zeroizing::new(vec![0u8; input.len() + block]);
    let mut written: c_int = 0;
    let ok = unsafe {
        EVP_CipherUpdate(
            ctx.0,
            out.as_mut_ptr(),
            &mut written,
            input.as_ptr(),
            in_len,
        )
    };
    if ok != 1 {
        return Err(CryptoError::Backend);
    }
    let first = usize::try_from(written).map_err(|_| CryptoError::Backend)?;
    let mut tail: c_int = 0;
    let ok = unsafe { EVP_CipherFinal_ex(ctx.0, out.as_mut_ptr().add(first), &mut tail) };
    if ok != 1 {
        return Err(if encrypt {
            CryptoError::Backend
        } else {
            CryptoError::BadPadding
        });
    }
    let total = first + usize::try_from(tail).map_err(|_| CryptoError::Backend)?;
    out.truncate(total);
    Ok(out)
}

/// Descifra `data` en modo CBC y quita el relleno PKCS#7.
pub fn cbc_decrypt(
    cipher: Cipher,
    key: &[u8],
    iv: &[u8],
    data: &[u8],
) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    run_cbc(cipher, key, iv, data, false)
}

/// Cifra `data` en modo CBC con relleno PKCS#7 (usado por los tests y fixtures sintéticos).
pub fn cbc_encrypt(
    cipher: Cipher,
    key: &[u8],
    iv: &[u8],
    data: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    run_cbc(cipher, key, iv, data, true).map(|mut v| std::mem::take(&mut *v))
}

/// Función hash para PBKDF2/HMAC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Digest {
    Sha1,
    Sha256,
}

impl Digest {
    fn raw(self) -> *const EvpMd {
        unsafe {
            match self {
                Digest::Sha1 => EVP_sha1(),
                Digest::Sha256 => EVP_sha256(),
            }
        }
    }

    /// Longitud de salida en bytes.
    pub fn len(self) -> usize {
        match self {
            Digest::Sha1 => 20,
            Digest::Sha256 => 32,
        }
    }

    /// Siempre `false`; existe para satisfacer la convención `len`/`is_empty`.
    pub fn is_empty(self) -> bool {
        false
    }
}

/// PBKDF2-HMAC con el hash indicado. `iterations` debe estar en `1..=i32::MAX`.
pub fn pbkdf2(
    digest: Digest,
    password: &[u8],
    salt: &[u8],
    iterations: u32,
    out_len: usize,
) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    let iter = c_int::try_from(iterations).map_err(|_| CryptoError::BadInput)?;
    let pass_len = c_int::try_from(password.len()).map_err(|_| CryptoError::BadInput)?;
    let salt_len = c_int::try_from(salt.len()).map_err(|_| CryptoError::BadInput)?;
    let key_len = c_int::try_from(out_len).map_err(|_| CryptoError::BadInput)?;
    if iter < 1 || out_len == 0 {
        return Err(CryptoError::BadInput);
    }
    let mut out = Zeroizing::new(vec![0u8; out_len]);
    let ok = unsafe {
        PKCS5_PBKDF2_HMAC(
            password.as_ptr().cast(),
            pass_len,
            salt.as_ptr(),
            salt_len,
            iter,
            digest.raw(),
            key_len,
            out.as_mut_ptr(),
        )
    };
    if ok != 1 {
        return Err(CryptoError::Backend);
    }
    Ok(out)
}

/// Resumen SHA-1 o SHA-256 de `data`.
pub fn hash(digest: Digest, data: &[u8]) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    let mut out = Zeroizing::new(vec![0u8; digest.len()]);
    let mut len: c_uint = 0;
    let ok = unsafe {
        EVP_Digest(
            data.as_ptr().cast(),
            data.len(),
            out.as_mut_ptr(),
            &mut len,
            digest.raw(),
            std::ptr::null_mut(),
        )
    };
    if ok != 1 || usize::try_from(len).ok() != Some(digest.len()) {
        return Err(CryptoError::Backend);
    }
    Ok(out)
}

/// HMAC con el hash indicado.
pub fn hmac(digest: Digest, key: &[u8], data: &[u8]) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    let key_len = c_int::try_from(key.len()).map_err(|_| CryptoError::BadInput)?;
    let mut out = Zeroizing::new(vec![0u8; digest.len()]);
    let mut len: c_uint = 0;
    let res = unsafe {
        HMAC(
            digest.raw(),
            key.as_ptr().cast(),
            key_len,
            data.as_ptr(),
            data.len(),
            out.as_mut_ptr(),
            &mut len,
        )
    };
    if res.is_null() || usize::try_from(len).ok() != Some(digest.len()) {
        return Err(CryptoError::Backend);
    }
    Ok(out)
}

/// Rellena `buf` con bytes aleatorios criptográficamente seguros.
pub fn random_bytes(buf: &mut [u8]) -> Result<(), CryptoError> {
    let len = c_int::try_from(buf.len()).map_err(|_| CryptoError::BadInput)?;
    if unsafe { RAND_bytes(buf.as_mut_ptr(), len) } != 1 {
        return Err(CryptoError::Backend);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn unhex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap_or(0))
            .collect()
    }

    #[test]
    fn pbkdf2_sha1_rfc6070() {
        let out = pbkdf2(Digest::Sha1, b"password", b"salt", 1, 20).unwrap();
        assert_eq!(hex(&out), "0c60c80f961f0e71f3a9b524af6012062fe037a6");
        let out = pbkdf2(Digest::Sha1, b"password", b"salt", 4096, 20).unwrap();
        assert_eq!(hex(&out), "4b007901b765489abead49d926f721d065a429c1");
    }

    #[test]
    fn pbkdf2_sha256_known_answer() {
        let out = pbkdf2(Digest::Sha256, b"password", b"salt", 1, 32).unwrap();
        assert_eq!(
            hex(&out),
            "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b"
        );
    }

    #[test]
    fn hashes_and_hmac_known_answers() {
        assert_eq!(
            hex(&hash(Digest::Sha1, b"abc").unwrap()),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hex(&hash(Digest::Sha256, b"abc").unwrap()),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // RFC 2202, caso 2.
        assert_eq!(
            hex(&hmac(Digest::Sha1, b"Jefe", b"what do ya want for nothing?").unwrap()),
            "effcdf6ae5eb2fa2d27416d5f184df9c259a7c79"
        );
    }

    #[test]
    fn aes128_cbc_nist_vector_and_roundtrip() {
        // NIST SP 800-38A F.2.1 (primer bloque) + relleno PKCS#7 añadido por OpenSSL.
        let key = unhex("2b7e151628aed2a6abf7158809cf4f3c");
        let iv = unhex("000102030405060708090a0b0c0d0e0f");
        let plain = unhex("6bc1bee22e409f96e93d7e117393172a");
        let ct = cbc_encrypt(Cipher::Aes128Cbc, &key, &iv, &plain).unwrap();
        assert_eq!(hex(&ct[..16]), "7649abac8119b246cee98e9b12e9197d");
        assert_eq!(ct.len(), 32);
        let back = cbc_decrypt(Cipher::Aes128Cbc, &key, &iv, &ct).unwrap();
        assert_eq!(&back[..], &plain[..]);
    }

    #[test]
    fn aes256_and_3des_roundtrip() {
        let key = [7u8; 32];
        let iv = [9u8; 16];
        let ct = cbc_encrypt(Cipher::Aes256Cbc, &key, &iv, b"secreto sintetico").unwrap();
        let pt = cbc_decrypt(Cipher::Aes256Cbc, &key, &iv, &ct).unwrap();
        assert_eq!(&pt[..], b"secreto sintetico");

        let key = unhex("0123456789abcdef23456789abcdef01456789abcdef0123");
        let iv = [1u8; 8];
        assert_eq!(Cipher::TripleDesCbc.key_len(), 24);
        assert_eq!(Cipher::TripleDesCbc.iv_len(), 8);
        let ct = cbc_encrypt(Cipher::TripleDesCbc, &key, &iv, b"x").unwrap();
        assert_eq!(ct.len(), 8);
        let pt = cbc_decrypt(Cipher::TripleDesCbc, &key, &iv, &ct).unwrap();
        assert_eq!(&pt[..], b"x");
    }

    #[test]
    fn rejects_bad_sizes_and_wrong_key() {
        let key = [1u8; 16];
        let iv = [2u8; 16];
        assert_eq!(
            cbc_decrypt(Cipher::Aes128Cbc, &key[..15], &iv, &[0u8; 16]).unwrap_err(),
            CryptoError::BadKeyOrIv
        );
        assert_eq!(
            cbc_decrypt(Cipher::Aes128Cbc, &key, &iv, &[0u8; 15]).unwrap_err(),
            CryptoError::BadInput
        );
        assert_eq!(
            cbc_decrypt(Cipher::Aes128Cbc, &key, &iv, &[]).unwrap_err(),
            CryptoError::BadInput
        );
        let ct = cbc_encrypt(Cipher::Aes128Cbc, &key, &iv, b"hola mundo").unwrap();
        // Con otra clave el relleno casi nunca es válido; si lo fuera, el texto difiere.
        match cbc_decrypt(Cipher::Aes128Cbc, &[3u8; 16], &iv, &ct) {
            Err(e) => assert_eq!(e, CryptoError::BadPadding),
            Ok(pt) => assert_ne!(&pt[..], b"hola mundo"),
        }
    }

    #[test]
    fn random_bytes_fills_buffer() {
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        random_bytes(&mut a).unwrap();
        random_bytes(&mut b).unwrap();
        assert_ne!(a, b);
    }
}
