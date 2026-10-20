//! Server authentication for the TLS 1.3 client: X.509 parsing, chain validation, host name
//! matching and `CertificateVerify` checking.
//!
//! Design rules
//! - Fail closed: every parse problem, unknown algorithm or missing field is an error.
//! - Signature mathematics is NOT implemented here. It is delegated to the RustCrypto crates
//!   (`rsa`, `p256`, `p384`) in `verify_signature`, the only function that touches them.
//! - Trust anchors come from the `webpki-roots` crate (Mozilla root store).
//!
//! Known limitations (documented on purpose, none of them silently weaken a *successful* check):
//! - No revocation checking (CRL / OCSP / stapling).
//! - Name constraints and unknown critical extensions are ignored.
//! - Validity dates are only checked when the caller supplies a Unix time (the OS clock is not wired in yet).
//! - Ed25519, P-521 and RSASSA-PSS certificate signatures are not supported (such servers are rejected).

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use sha2::{Digest, Sha256, Sha384, Sha512};

// ---------------------------------------------------------------------------------------------
// OIDs (DER content bytes)
// ---------------------------------------------------------------------------------------------

const OID_RSA_ENCRYPTION: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x01];
const OID_SHA256_RSA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0B];
const OID_SHA384_RSA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0C];
const OID_SHA512_RSA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0D];
const OID_ECDSA_SHA256: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x04, 0x03, 0x02];
const OID_ECDSA_SHA384: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x04, 0x03, 0x03];
const OID_ECDSA_SHA512: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x04, 0x03, 0x04];
const OID_EC_PUBLIC_KEY: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x02, 0x01];
const OID_P256: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x03, 0x01, 0x07];
const OID_P384: &[u8] = &[0x2B, 0x81, 0x04, 0x00, 0x22];
const OID_EXT_SAN: &[u8] = &[0x55, 0x1D, 0x11];
const OID_EXT_BASIC_CONSTRAINTS: &[u8] = &[0x55, 0x1D, 0x13];
const OID_EXT_KEY_USAGE: &[u8] = &[0x55, 0x1D, 0x0F];
const OID_EXT_EKU: &[u8] = &[0x55, 0x1D, 0x25];
const OID_EKU_SERVER_AUTH: &[u8] = &[0x2B, 0x06, 0x01, 0x05, 0x05, 0x07, 0x03, 0x01];
const OID_EKU_ANY: &[u8] = &[0x55, 0x1D, 0x25, 0x00];

const MAX_CHAIN: usize = 8;
const MIN_RSA_BYTES: usize = 256; // 2048 bit

// ---------------------------------------------------------------------------------------------
// Minimal DER reader
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Tlv<'a> {
    tag: u8,
    content: &'a [u8],
    full: &'a [u8],
}

/// Reads one TLV from the front of `data`; returns it and the remaining bytes.
fn read_tlv(data: &[u8]) -> Option<(Tlv<'_>, &[u8])> {
    if data.len() < 2 {
        return None;
    }
    let tag = data[0];
    if tag & 0x1F == 0x1F {
        return None; // multi-byte tags are never used in X.509 structures we parse
    }
    let b = data[1] as usize;
    let (len, hdr) = if b < 0x80 {
        (b, 2usize)
    } else {
        let n = b & 0x7F;
        if n == 0 || n > 3 || data.len() < 2 + n {
            return None;
        }
        let mut l = 0usize;
        for i in 0..n {
            l = (l << 8) | data[2 + i] as usize;
        }
        (l, 2 + n)
    };
    if data.len() < hdr + len {
        return None;
    }
    Some((
        Tlv { tag, content: &data[hdr..hdr + len], full: &data[..hdr + len] },
        &data[hdr + len..],
    ))
}

fn expect_tag<'a>(data: &'a [u8], tag: u8) -> Option<(Tlv<'a>, &'a [u8])> {
    let (t, rest) = read_tlv(data)?;
    if t.tag != tag {
        return None;
    }
    Some((t, rest))
}

// ---------------------------------------------------------------------------------------------
// X.509 certificate
// ---------------------------------------------------------------------------------------------

struct Cert<'a> {
    tbs: &'a [u8],
    sig_alg_oid: &'a [u8],
    signature: &'a [u8],
    issuer: &'a [u8],
    subject: &'a [u8],
    not_before: Option<u64>,
    not_after: Option<u64>,
    spki: Spki<'a>,
    san_dns: Vec<&'a [u8]>,
    san_ip: Vec<&'a [u8]>,
    bc_present: bool,
    is_ca: bool,
    path_len: Option<u32>,
    key_usage: Option<u8>,
    eku_ok: bool,
}

#[derive(Clone, Copy)]
struct Spki<'a> {
    alg_oid: &'a [u8],
    /// Content of the algorithm parameters (for EC keys: the curve OID bytes).
    params: &'a [u8],
    /// BIT STRING payload without the leading "unused bits" byte.
    key: &'a [u8],
}

fn parse_spki_inner(inner: &[u8]) -> Option<Spki<'_>> {
    let (alg, rest) = expect_tag(inner, 0x30)?;
    let (key, rest2) = expect_tag(rest, 0x03)?;
    if !rest2.is_empty() || key.content.is_empty() || key.content[0] != 0 {
        return None;
    }
    let (oid, after_oid) = expect_tag(alg.content, 0x06)?;
    let params = match read_tlv(after_oid) {
        Some((p, _)) if p.tag == 0x06 => p.content, // EC named curve
        _ => &[],                                   // RSA: NULL or absent
    };
    Some(Spki { alg_oid: oid.content, params, key: &key.content[1..] })
}

/// Trust-anchor SPKI as stored by `webpki-roots` may or may not include the outer SEQUENCE header.
/// Both forms are accepted: the header is recognised because it wraps *everything*.
fn parse_anchor_spki(bytes: &[u8]) -> Option<Spki<'_>> {
    if let Some((t, rest)) = read_tlv(bytes) {
        if rest.is_empty() && t.tag == 0x30 {
            if let Some(s) = parse_spki_inner(t.content) {
                return Some(s);
            }
        }
    }
    parse_spki_inner(bytes)
}

/// Normalises a DER Name to its content bytes (without the outer SEQUENCE header).
fn name_contents(bytes: &[u8]) -> &[u8] {
    if let Some((t, rest)) = read_tlv(bytes) {
        if rest.is_empty() && t.tag == 0x30 {
            return t.content;
        }
    }
    bytes
}

fn digits(s: &[u8], at: usize, n: usize) -> Option<i64> {
    let mut v = 0i64;
    for i in 0..n {
        let c = *s.get(at + i)?;
        if !c.is_ascii_digit() {
            return None;
        }
        v = v * 10 + (c - b'0') as i64;
    }
    Some(v)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// UTCTime (`YYMMDDHHMMSSZ`) or GeneralizedTime (`YYYYMMDDHHMMSSZ`) -> Unix seconds.
fn parse_time(t: Tlv<'_>) -> Option<u64> {
    let s = t.content;
    let (year, off) = match t.tag {
        0x17 if s.len() == 13 => {
            let yy = digits(s, 0, 2)?;
            (if yy >= 50 { 1900 + yy } else { 2000 + yy }, 2)
        }
        0x18 if s.len() == 15 => (digits(s, 0, 4)?, 4),
        _ => return None,
    };
    if *s.last()? != b'Z' {
        return None;
    }
    let month = digits(s, off, 2)?;
    let day = digits(s, off + 2, 2)?;
    let hour = digits(s, off + 4, 2)?;
    let min = digits(s, off + 6, 2)?;
    let sec = digits(s, off + 8, 2)?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || min > 59 || sec > 60 {
        return None;
    }
    let secs = days_from_civil(year, month, day) * 86_400 + hour * 3600 + min * 60 + sec;
    if secs < 0 {
        None
    } else {
        Some(secs as u64)
    }
}

fn parse_cert(der: &[u8]) -> Option<Cert<'_>> {
    if der.len() > 32 * 1024 {
        return None;
    }
    let (cert, rest) = expect_tag(der, 0x30)?;
    if !rest.is_empty() {
        return None;
    }
    let (tbs, after_tbs) = expect_tag(cert.content, 0x30)?;
    let (sig_alg, after_alg) = expect_tag(after_tbs, 0x30)?;
    let (sig_bits, trailing) = expect_tag(after_alg, 0x03)?;
    if !trailing.is_empty() || sig_bits.content.is_empty() || sig_bits.content[0] != 0 {
        return None;
    }
    let (sig_oid, _) = expect_tag(sig_alg.content, 0x06)?;

    let mut p = tbs.content;
    if let Some((v, r)) = read_tlv(p) {
        if v.tag == 0xA0 {
            p = r; // [0] version
        }
    }
    let (_serial, p) = expect_tag(p, 0x02)?;
    let (_sig, p) = expect_tag(p, 0x30)?;
    let (issuer, p) = expect_tag(p, 0x30)?;
    let (validity, p) = expect_tag(p, 0x30)?;
    let (subject, p) = expect_tag(p, 0x30)?;
    let (spki_tlv, mut p) = expect_tag(p, 0x30)?;
    let spki = parse_spki_inner(spki_tlv.content)?;

    let (nb, v_rest) = read_tlv(validity.content)?;
    let (na, _) = read_tlv(v_rest)?;

    let mut cert_out = Cert {
        tbs: tbs.full,
        sig_alg_oid: sig_oid.content,
        signature: &sig_bits.content[1..],
        issuer: issuer.content,
        subject: subject.content,
        not_before: parse_time(nb),
        not_after: parse_time(na),
        spki,
        san_dns: Vec::new(),
        san_ip: Vec::new(),
        bc_present: false,
        is_ca: false,
        path_len: None,
        key_usage: None,
        eku_ok: true,
    };
    if cert_out.not_before.is_none() || cert_out.not_after.is_none() {
        return None;
    }

    // optional issuerUniqueID [1], subjectUniqueID [2], then extensions [3]
    while !p.is_empty() {
        let (t, r) = read_tlv(p)?;
        p = r;
        if t.tag != 0xA3 {
            continue;
        }
        let (exts, _) = expect_tag(t.content, 0x30)?;
        let mut e = exts.content;
        while !e.is_empty() {
            let (ext, r2) = expect_tag(e, 0x30)?;
            e = r2;
            let (oid, mut q) = expect_tag(ext.content, 0x06)?;
            if let Some((b, r3)) = read_tlv(q) {
                if b.tag == 0x01 {
                    q = r3; // critical flag (not enforced)
                }
            }
            let (value, _) = expect_tag(q, 0x04)?;
            parse_extension(&mut cert_out, oid.content, value.content)?;
        }
    }
    Some(cert_out)
}

fn parse_extension<'a>(cert: &mut Cert<'a>, oid: &[u8], value: &'a [u8]) -> Option<()> {
    if oid == OID_EXT_SAN {
        let (seq, _) = expect_tag(value, 0x30)?;
        let mut g = seq.content;
        while !g.is_empty() {
            let (name, r) = read_tlv(g)?;
            g = r;
            match name.tag {
                0x82 => cert.san_dns.push(name.content),
                0x87 => cert.san_ip.push(name.content),
                _ => {}
            }
        }
    } else if oid == OID_EXT_BASIC_CONSTRAINTS {
        let (seq, _) = expect_tag(value, 0x30)?;
        cert.bc_present = true;
        let mut c = seq.content;
        if let Some((b, r)) = read_tlv(c) {
            if b.tag == 0x01 {
                cert.is_ca = b.content.first().map_or(false, |v| *v != 0);
                c = r;
            }
        }
        if let Some((n, _)) = read_tlv(c) {
            if n.tag == 0x02 && !n.content.is_empty() && n.content.len() <= 4 && n.content[0] & 0x80 == 0 {
                let mut v = 0u32;
                for b in n.content {
                    v = (v << 8) | *b as u32;
                }
                cert.path_len = Some(v);
            }
        }
    } else if oid == OID_EXT_KEY_USAGE {
        let (bits, _) = expect_tag(value, 0x03)?;
        cert.key_usage = bits.content.get(1).copied();
    } else if oid == OID_EXT_EKU {
        let (seq, _) = expect_tag(value, 0x30)?;
        let mut ok = false;
        let mut g = seq.content;
        while !g.is_empty() {
            let (o, r) = read_tlv(g)?;
            g = r;
            if o.tag == 0x06 && (o.content == OID_EKU_SERVER_AUTH || o.content == OID_EKU_ANY) {
                ok = true;
            }
        }
        cert.eku_ok = ok;
    }
    Some(())
}

// ---------------------------------------------------------------------------------------------
// Host name matching (RFC 6125)
// ---------------------------------------------------------------------------------------------

fn parse_ipv4(host: &str) -> Option<[u8; 4]> {
    let mut out = [0u8; 4];
    let mut n = 0;
    for part in host.split('.') {
        if n >= 4 || part.is_empty() || part.len() > 3 || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        out[n] = part.parse::<u16>().ok().filter(|v| *v <= 255)? as u8;
        n += 1;
    }
    if n == 4 {
        Some(out)
    } else {
        None
    }
}

fn dns_name_matches(pattern: &[u8], host: &str) -> bool {
    let Ok(p) = core::str::from_utf8(pattern) else {
        return false;
    };
    if !p.is_ascii() {
        return false;
    }
    let p = p.to_ascii_lowercase();
    let p = p.trim_end_matches('.');
    if let Some(rest) = p.strip_prefix("*.") {
        // The wildcard stands for exactly one label and the rest must still contain a dot (no `*.com`).
        if !rest.contains('.') || rest.contains('*') {
            return false;
        }
        return match host.split_once('.') {
            Some((first, tail)) => !first.is_empty() && tail == rest,
            None => false,
        };
    }
    !p.contains('*') && p == host
}

fn host_matches(host: &str, cert: &Cert<'_>) -> bool {
    let h = host.trim_end_matches('.').to_ascii_lowercase();
    if h.is_empty() {
        return false;
    }
    if let Some(ip) = parse_ipv4(&h) {
        return cert.san_ip.iter().any(|b| *b == &ip[..]);
    }
    // The Common Name is deliberately ignored: modern certificates must carry a subjectAltName.
    cert.san_dns.iter().any(|n| dns_name_matches(n, &h))
}

// ---------------------------------------------------------------------------------------------
// Signature verification (the only place that uses the crypto crates)
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Hash {
    S256,
    S384,
    S512,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SigKind {
    RsaPkcs1(Hash),
    RsaPss(Hash),
    Ecdsa(Hash),
}

fn digest(h: Hash, data: &[u8]) -> Vec<u8> {
    match h {
        Hash::S256 => Sha256::digest(data).to_vec(),
        Hash::S384 => Sha384::digest(data).to_vec(),
        Hash::S512 => Sha512::digest(data).to_vec(),
    }
}

fn cert_sig_kind(oid: &[u8]) -> Option<SigKind> {
    if oid == OID_SHA256_RSA {
        Some(SigKind::RsaPkcs1(Hash::S256))
    } else if oid == OID_SHA384_RSA {
        Some(SigKind::RsaPkcs1(Hash::S384))
    } else if oid == OID_SHA512_RSA {
        Some(SigKind::RsaPkcs1(Hash::S512))
    } else if oid == OID_ECDSA_SHA256 {
        Some(SigKind::Ecdsa(Hash::S256))
    } else if oid == OID_ECDSA_SHA384 {
        Some(SigKind::Ecdsa(Hash::S384))
    } else if oid == OID_ECDSA_SHA512 {
        Some(SigKind::Ecdsa(Hash::S512))
    } else {
        None // SHA-1, MD5, RSASSA-PSS certificates, Ed25519 ... are rejected
    }
}

/// Converts a DER `ECDSA-Sig-Value` to fixed-size big-endian `r || s`.
fn ecdsa_der_to_raw(sig: &[u8], size: usize) -> Option<Vec<u8>> {
    let (seq, rest) = expect_tag(sig, 0x30)?;
    if !rest.is_empty() {
        return None;
    }
    let (r, rest2) = expect_tag(seq.content, 0x02)?;
    let (s, rest3) = expect_tag(rest2, 0x02)?;
    if !rest3.is_empty() {
        return None;
    }
    let mut out = alloc::vec![0u8; size * 2];
    for (idx, int) in [r.content, s.content].iter().enumerate() {
        let mut v: &[u8] = int;
        while v.len() > 1 && v[0] == 0 {
            v = &v[1..];
        }
        if v.len() > size {
            return None;
        }
        out[idx * size + (size - v.len())..(idx + 1) * size].copy_from_slice(v);
    }
    Some(out)
}

fn verify_signature(spki: &Spki<'_>, kind: SigKind, msg: &[u8], sig: &[u8]) -> bool {
    match kind {
        SigKind::RsaPkcs1(h) | SigKind::RsaPss(h) => {
            if spki.alg_oid != OID_RSA_ENCRYPTION {
                return false;
            }
            let Some((seq, rest)) = expect_tag(spki.key, 0x30) else {
                return false;
            };
            if !rest.is_empty() {
                return false;
            }
            let Some((n, after_n)) = expect_tag(seq.content, 0x02) else {
                return false;
            };
            let Some((e, tail)) = expect_tag(after_n, 0x02) else {
                return false;
            };
            if !tail.is_empty() {
                return false;
            }
            let mut n_bytes: &[u8] = n.content;
            while n_bytes.len() > 1 && n_bytes[0] == 0 {
                n_bytes = &n_bytes[1..];
            }
            if n_bytes.len() < MIN_RSA_BYTES {
                return false;
            }
            let key = match rsa::RsaPublicKey::new(rsa::BigUint::from_bytes_be(n_bytes), rsa::BigUint::from_bytes_be(e.content)) {
                Ok(k) => k,
                Err(_) => return false,
            };
            let hashed = digest(h, msg);
            let ok = match (kind, h) {
                (SigKind::RsaPkcs1(_), Hash::S256) => key.verify(rsa::Pkcs1v15Sign::new::<Sha256>(), &hashed, sig),
                (SigKind::RsaPkcs1(_), Hash::S384) => key.verify(rsa::Pkcs1v15Sign::new::<Sha384>(), &hashed, sig),
                (SigKind::RsaPkcs1(_), Hash::S512) => key.verify(rsa::Pkcs1v15Sign::new::<Sha512>(), &hashed, sig),
                (_, Hash::S256) => key.verify(rsa::Pss::new::<Sha256>(), &hashed, sig),
                (_, Hash::S384) => key.verify(rsa::Pss::new::<Sha384>(), &hashed, sig),
                (_, Hash::S512) => key.verify(rsa::Pss::new::<Sha512>(), &hashed, sig),
            };
            ok.is_ok()
        }
        SigKind::Ecdsa(h) => {
            if spki.alg_oid != OID_EC_PUBLIC_KEY {
                return false;
            }
            let hashed = digest(h, msg);
            if spki.params == OID_P256 {
                use p256::ecdsa::signature::hazmat::PrehashVerifier;
                let Some(raw) = ecdsa_der_to_raw(sig, 32) else {
                    return false;
                };
                let Ok(vk) = p256::ecdsa::VerifyingKey::from_sec1_bytes(spki.key) else {
                    return false;
                };
                let Ok(sg) = p256::ecdsa::Signature::from_slice(&raw) else {
                    return false;
                };
                vk.verify_prehash(&hashed, &sg).is_ok()
            } else if spki.params == OID_P384 {
                use p384::ecdsa::signature::hazmat::PrehashVerifier;
                let Some(raw) = ecdsa_der_to_raw(sig, 48) else {
                    return false;
                };
                let Ok(vk) = p384::ecdsa::VerifyingKey::from_sec1_bytes(spki.key) else {
                    return false;
                };
                let Ok(sg) = p384::ecdsa::Signature::from_slice(&raw) else {
                    return false;
                };
                vk.verify_prehash(&hashed, &sg).is_ok()
            } else {
                false
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------------------------

/// Validates the server certificate chain for `host`.
///
/// Checks: host name (subjectAltName), validity period (when `now_unix` is given), extended key usage,
/// issuer/subject linkage, CA flags and path length, every signature in the chain, and that the chain
/// ends in a trust anchor from the Mozilla root store.
pub fn verify_chain(chain: &[Vec<u8>], host: &str, now_unix: Option<u64>) -> Result<(), &'static str> {
    if chain.is_empty() || chain.len() > MAX_CHAIN {
        return Err("certificate chain size is invalid");
    }
    let mut certs: Vec<Cert<'_>> = Vec::new();
    for der in chain {
        certs.push(parse_cert(der).ok_or("certificate could not be parsed")?);
    }

    if !host_matches(host, &certs[0]) {
        return Err("certificate does not match the host name");
    }
    if !certs[0].eku_ok {
        return Err("certificate is not valid for server authentication");
    }
    if let Some(now) = now_unix {
        for c in certs.iter() {
            let (nb, na) = (c.not_before.unwrap_or(u64::MAX), c.not_after.unwrap_or(0));
            if now < nb {
                return Err("certificate is not valid yet");
            }
            if now > na {
                return Err("certificate has expired");
            }
        }
    }

    for i in 0..certs.len() - 1 {
        let child = &certs[i];
        let parent = &certs[i + 1];
        if parent.subject != child.issuer {
            return Err("certificate chain is not linked (issuer mismatch)");
        }
        if !(parent.bc_present && parent.is_ca) {
            return Err("an issuing certificate is not a CA");
        }
        if let Some(ku) = parent.key_usage {
            if ku & 0x04 == 0 {
                return Err("an issuing certificate may not sign certificates");
            }
        }
        if let Some(max) = parent.path_len {
            if (i as u32) > max {
                return Err("certificate path length constraint violated");
            }
        }
        let kind = cert_sig_kind(child.sig_alg_oid).ok_or("unsupported certificate signature algorithm")?;
        if !verify_signature(&parent.spki, kind, child.tbs, child.signature) {
            return Err("certificate signature is invalid");
        }
    }

    // The last certificate must be signed by (or be) a trust anchor.
    let top = &certs[certs.len() - 1];
    for ta in webpki_roots::TLS_SERVER_ROOTS.iter() {
        let anchor_subject = name_contents(ta.subject.as_ref());
        let Some(anchor_spki) = parse_anchor_spki(ta.subject_public_key_info.as_ref()) else {
            continue;
        };
        if anchor_subject == top.issuer {
            if let Some(kind) = cert_sig_kind(top.sig_alg_oid) {
                if verify_signature(&anchor_spki, kind, top.tbs, top.signature) {
                    return Ok(());
                }
            }
        }
        // The server may also send the root itself; accept it only if it is exactly a stored anchor.
        if anchor_subject == top.subject
            && anchor_spki.alg_oid == top.spki.alg_oid
            && anchor_spki.params == top.spki.params
            && anchor_spki.key == top.spki.key
        {
            return Ok(());
        }
    }
    Err("certificate chain does not lead to a trusted root")
}

/// Verifies the TLS 1.3 `CertificateVerify` signature made with the leaf certificate's key.
/// `transcript_hash` is the handshake hash up to and including the `Certificate` message.
pub fn verify_certificate_verify(
    leaf_der: &[u8],
    scheme: u16,
    transcript_hash: &[u8],
    signature: &[u8],
) -> Result<(), &'static str> {
    let leaf = parse_cert(leaf_der).ok_or("certificate could not be parsed")?;
    // Only the schemes advertised in the ClientHello are acceptable; RSA PKCS#1 v1.5 is forbidden in TLS 1.3.
    let (kind, curve): (SigKind, Option<&[u8]>) = match scheme {
        0x0403 => (SigKind::Ecdsa(Hash::S256), Some(OID_P256)),
        0x0503 => (SigKind::Ecdsa(Hash::S384), Some(OID_P384)),
        0x0804 => (SigKind::RsaPss(Hash::S256), None),
        0x0805 => (SigKind::RsaPss(Hash::S384), None),
        0x0806 => (SigKind::RsaPss(Hash::S512), None),
        _ => return Err("server chose an unsupported signature scheme"),
    };
    if let Some(c) = curve {
        if leaf.spki.params != c {
            return Err("signature scheme does not match the certificate key");
        }
    }
    let mut msg: Vec<u8> = Vec::with_capacity(64 + 34 + transcript_hash.len());
    msg.extend_from_slice(&[0x20u8; 64]);
    msg.extend_from_slice(b"TLS 1.3, server CertificateVerify");
    msg.push(0);
    msg.extend_from_slice(transcript_hash);
    if verify_signature(&leaf.spki, kind, &msg, signature) {
        Ok(())
    } else {
        Err("CertificateVerify signature is invalid")
    }
}

/// Human readable form of the host used in diagnostics (no allocation of untrusted data beyond the host).
pub fn describe(host: &str, reason: &str) -> String {
    alloc::format!("TLS authentication failed for {}: {}", host, reason)
}
