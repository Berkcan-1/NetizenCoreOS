//! nc-pkg testleri (F-02, F-11).
//!
//! DURUM: Bu testler yazıldı ancak yazıldığı ortamda `cargo` yoktu; DERLENMEDİ ve
//! ÇALIŞTIRILMADI. İlk iş: `cargo test -p nc-pkg`.
use super::*;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use ed25519_dalek::{Signer, SigningKey};

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::new();
    for b in bytes {
        s.push(DIGITS[(b >> 4) as usize] as char);
        s.push(DIGITS[(b & 15) as usize] as char);
    }
    s
}

fn unhex(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    assert!(b.len() % 2 == 0);
    let mut out = Vec::new();
    for pair in b.chunks(2) {
        let nib = |c: u8| match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            _ => panic!("hex"),
        };
        out.push((nib(pair[0]) << 4) | nib(pair[1]));
    }
    out
}

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn pub_hex(sk: &SigningKey) -> String {
    hex(sk.verifying_key().as_bytes())
}

fn signed_text(publisher_pub_hex: &str, payload_hash_hex: &str) -> String {
    format!(
        "id=nc.user.test\nname=Test\nversion=1.0.0\nversion_code=1\nicon=generic\n\
         width_units=30\nheight_units=20\ntarget_arch=x86_64-unknown-none\n\
         publisher_key_id=ed25519:{}\npayload_hash={}\ncapabilities=filesystem.user.read\n",
        publisher_pub_hex, payload_hash_hex
    )
}

/// Ham NTZ2 paketi kurar. `manifest_full` tam manifest metnidir (imza dahil / hariç, çağıran belirler),
/// `header_hash` başlıktaki hash.
fn assemble(manifest_full: &str, payload: &[u8], header_hash: &[u8; 32]) -> Vec<u8> {
    let m = manifest_full.as_bytes();
    let m_off = NTZ2_HEADER_LEN;
    let p_off = m_off + m.len();
    let mut out = Vec::new();
    out.extend_from_slice(b"NTZ2");
    for v in [64u32, 2, 0, m_off as u32, m.len() as u32, p_off as u32, payload.len() as u32] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(header_hash);
    assert_eq!(out.len(), NTZ2_HEADER_LEN);
    out.extend_from_slice(m);
    out.extend_from_slice(payload);
    out
}

/// Dürüst, doğru imzalı paket: `signer` imzalar, manifestte `claimed_pub` yazar.
fn build_signed(signer: &SigningKey, claimed_pub: &str, payload: &[u8]) -> Vec<u8> {
    let h = compute_sha256(payload);
    let text = signed_text(claimed_pub, &hex(&h));
    let sig = signer.sign(text.as_bytes());
    let full = format!("{}sig={}\n", text, hex(&sig.to_bytes()));
    assemble(&full, payload, &h)
}

const PAYLOAD: &[u8] = b"NCBM-test-payload-bytes-0123456789";

fn trusted(sk: &SigningKey) -> Vec<String> {
    vec![pub_hex(sk)]
}

fn parse_with(bytes: &[u8], trust: &[String], revoked: &[String]) -> Result<NtzcPackage, PackageError> {
    let t: Vec<&str> = trust.iter().map(|s| s.as_str()).collect();
    let r: Vec<&str> = revoked.iter().map(|s| s.as_str()).collect();
    NtzcPackage::parse_with_keys(bytes, &t, &r)
}

fn find(hay: &[u8], needle: &[u8]) -> usize {
    hay.windows(needle.len()).position(|w| w == needle).expect("needle")
}

// ---- kabul: geçerli paket ---------------------------------------------------------------

#[test]
fn valid_signed_package_is_verified() {
    let sk = key(7);
    let bytes = build_signed(&sk, &pub_hex(&sk), PAYLOAD);
    let pkg = parse_with(&bytes, &trusted(&sk), &[]).expect("geçerli paket kabul edilmeli");
    assert!(pkg.is_verified);
    assert_eq!(pkg.manifest.id, "nc.user.test");
    assert_eq!(pkg.payload, PAYLOAD);
    assert_eq!(pkg.uid, 1001);
    assert_eq!(pkg.permissions, 0);
}

// ---- ret: raporun F-02 kabul kriterleri ---------------------------------------------------

#[test]
fn one_byte_changed_in_manifest_is_rejected() {
    let sk = key(7);
    let mut bytes = build_signed(&sk, &pub_hex(&sk), PAYLOAD);
    let i = find(&bytes, b"version=1.0.0") + "version=1.0.".len();
    bytes[i] = b'1'; // 1.0.0 -> 1.0.1
    assert_eq!(parse_with(&bytes, &trusted(&sk), &[]).unwrap_err(), PackageError::BadSignature);
}

#[test]
fn payload_changed_without_hash_update_is_rejected() {
    let sk = key(7);
    let mut bytes = build_signed(&sk, &pub_hex(&sk), PAYLOAD);
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    assert_eq!(
        parse_with(&bytes, &trusted(&sk), &[]).unwrap_err(),
        PackageError::PayloadHashMismatch
    );
}

/// EN ÖNEMLİ TEST: saldırgan payload'ı VE başlıktaki hash'i değiştirir, manifest/imza eski kalır.
/// Eski (hatalı) kod bunu "verified" sayıyordu.
#[test]
fn payload_and_header_hash_changed_but_old_manifest_and_signature_is_rejected() {
    let sk = key(7);
    let good = build_signed(&sk, &pub_hex(&sk), PAYLOAD);
    // Manifest + imza bölgesi iyi paketten aynen alınır.
    let m_len = u32::from_le_bytes([good[20], good[21], good[22], good[23]]) as usize;
    let manifest_full = core::str::from_utf8(&good[NTZ2_HEADER_LEN..NTZ2_HEADER_LEN + m_len]).unwrap();
    let evil_payload = b"NCBM-EVIL-payload-that-writes-PWNED";
    let evil = assemble(manifest_full, evil_payload, &compute_sha256(evil_payload));
    assert_eq!(
        parse_with(&evil, &trusted(&sk), &[]).unwrap_err(),
        PackageError::PayloadHashMismatch
    );
}

#[test]
fn trusted_key_id_but_signed_by_another_key_is_rejected() {
    let trusted_key = key(7);
    let attacker = key(9);
    // publisher_key_id güvenilir anahtarı iddia ediyor, imzayı ise saldırgan atıyor.
    let bytes = build_signed(&attacker, &pub_hex(&trusted_key), PAYLOAD);
    assert_eq!(
        parse_with(&bytes, &trusted(&trusted_key), &[]).unwrap_err(),
        PackageError::BadSignature
    );
}

#[test]
fn zero_and_missing_and_empty_signature_are_rejected() {
    let sk = key(7);
    let h = compute_sha256(PAYLOAD);
    let text = signed_text(&pub_hex(&sk), &hex(&h));

    let zero_sig = format!("{}sig={}\n", text, "0".repeat(128));
    let p = assemble(&zero_sig, PAYLOAD, &h);
    assert_eq!(parse_with(&p, &trusted(&sk), &[]).unwrap_err(), PackageError::BadSignature);

    let empty_sig = format!("{}sig=\n", text);
    let p = assemble(&empty_sig, PAYLOAD, &h);
    assert_eq!(parse_with(&p, &trusted(&sk), &[]).unwrap_err(), PackageError::BadSignature);

    let no_sig = text.clone();
    let p = assemble(&no_sig, PAYLOAD, &h);
    assert_eq!(parse_with(&p, &trusted(&sk), &[]).unwrap_err(), PackageError::MissingSignature);
}

#[test]
fn content_after_signature_line_is_rejected() {
    let sk = key(7);
    let h = compute_sha256(PAYLOAD);
    let text = signed_text(&pub_hex(&sk), &hex(&h));
    let sig = sk.sign(text.as_bytes());
    // İmzadan sonra eklenmiş yetki satırı: imza geçerli olsa bile paket reddedilmeli.
    let full = format!("{}sig={}\ncapabilities=package.install\n", text, hex(&sig.to_bytes()));
    let p = assemble(&full, PAYLOAD, &h);
    assert_eq!(parse_with(&p, &trusted(&sk), &[]).unwrap_err(), PackageError::BadManifest);
}

#[test]
fn duplicate_signature_lines_are_rejected() {
    let sk = key(7);
    let h = compute_sha256(PAYLOAD);
    let text = signed_text(&pub_hex(&sk), &hex(&h));
    let sig = hex(&sk.sign(text.as_bytes()).to_bytes());
    let full = format!("{}sig={}\nsig={}\n", text, sig, sig);
    let p = assemble(&full, PAYLOAD, &h);
    assert_eq!(parse_with(&p, &trusted(&sk), &[]).unwrap_err(), PackageError::BadManifest);
}

#[test]
fn valid_signature_from_untrusted_publisher_is_rejected() {
    let sk = key(7);
    let other = key(9);
    let bytes = build_signed(&sk, &pub_hex(&sk), PAYLOAD);
    assert_eq!(
        parse_with(&bytes, &trusted(&other), &[]).unwrap_err(),
        PackageError::UntrustedPublisher
    );
}

#[test]
fn revoked_key_wins_over_trusted_list() {
    let sk = key(7);
    let bytes = build_signed(&sk, &pub_hex(&sk), PAYLOAD);
    assert_eq!(
        parse_with(&bytes, &trusted(&sk), &trusted(&sk)).unwrap_err(),
        PackageError::RevokedPublisher
    );
}

#[test]
fn manifest_hash_must_match_even_if_header_hash_matches_payload() {
    // İmzalı manifest başka bir payload hash'i bağlıyor; başlık gerçek payload ile tutarlı.
    let sk = key(7);
    let other_payload = b"some-other-payload";
    let h_real = compute_sha256(PAYLOAD);
    let text = signed_text(&pub_hex(&sk), &hex(&compute_sha256(other_payload)));
    let sig = hex(&sk.sign(text.as_bytes()).to_bytes());
    let full = format!("{}sig={}\n", text, sig);
    let p = assemble(&full, PAYLOAD, &h_real);
    assert_eq!(
        parse_with(&p, &trusted(&sk), &[]).unwrap_err(),
        PackageError::PayloadHashMismatch
    );
}

#[test]
fn truncated_and_garbage_inputs_do_not_panic() {
    let sk = key(7);
    let good = build_signed(&sk, &pub_hex(&sk), PAYLOAD);
    for n in 0..good.len() {
        let _ = parse_with(&good[..n], &trusted(&sk), &[]);
    }
    let _ = parse_with(&[0u8; 64], &trusted(&sk), &[]);
    let _ = parse_with(&[0xFFu8; 200], &trusted(&sk), &[]);
}

#[test]
fn production_trust_list_rejects_the_test_key() {
    // Üretim listesinde test anahtarı (seed=[7;32]) olmamalı.
    let sk = key(7);
    let bytes = build_signed(&sk, &pub_hex(&sk), PAYLOAD);
    assert!(NtzcPackage::parse(&bytes).is_none());
}

#[test]
fn production_trust_list_does_not_contain_known_secret_keys() {
    // 32 sıfır baytlık seed'in açık anahtarı (eski yerleşik "güvenilir" anahtar) ve RFC 8032
    // test vektörü 1'in açık anahtarı: özel anahtarları herkesçe bilinir; asla güvenilir olmamalı.
    let known_bad = [
        "3b6a27bcceb6a42d62a3a8d02a6f0d73653215771de243a63ac048a18b59da29",
        "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
    ];
    for t in TRUSTED_PUBLISHERS_HEX {
        for bad in known_bad {
            assert_ne!(t.trim(), bad);
        }
    }
}

// ---- v1 (imzasız) -------------------------------------------------------------------------

fn build_v1(uid: u32, perms: u32, manifest: &str, payload: &[u8]) -> Vec<u8> {
    let m = manifest.as_bytes();
    let mut out = Vec::new();
    out.extend_from_slice(b"NTZC");
    for v in [1u32, uid, perms, m.len() as u32, (28 + m.len()) as u32, payload.len() as u32] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(m);
    out.extend_from_slice(payload);
    out
}

#[test]
fn v1_package_is_never_verified_and_header_claims_are_ignored() {
    let bytes = build_v1(1000, 0xFF, "id=nc.user.x\nname=X\n", b"payload");
    let pkg = NtzcPackage::parse(&bytes).expect("v1 ayrıştırılır");
    assert!(!pkg.is_verified);
    assert_eq!(pkg.uid, 1001);
    assert_eq!(pkg.permissions, 0);
    assert!(!pkg.is_system());
}

#[test]
fn app_id_with_path_characters_is_rejected() {
    let bytes = build_v1(1001, 0, "id=a.b/../../etc\nname=X\n", b"payload");
    assert!(NtzcPackage::parse(&bytes).is_none());
    assert!(manifest::valid_app_id("nc.user.paint"));
    assert!(!manifest::valid_app_id(""));
    assert!(!manifest::valid_app_id("a b"));
    assert!(!manifest::valid_app_id("a/b"));
    assert!(!manifest::valid_app_id(&"a".repeat(65)));
}

// ---- Python SDK ile biçim uyumu -----------------------------------------------------------

/// `sdk/build-ntzc.py` ile (seed = 32 x 0x07, YALNIZCA test anahtarı) üretilmiş gerçek paket.
const SDK_FIXTURE_HEX: &str = concat!(
    "4e545a3240000000020000000000000040000000e701000027020000220000003163948d5468a50ca8c899f178553687",
    "d3f16328a27f8d6a035e3c5ae7f1f8e969643d6e632e757365722e666978747572650a6e616d653d466978747572650a",
    "76657273696f6e3d312e302e300a76657273696f6e5f636f64653d310a69636f6e3d67656e657269630a77696474685f",
    "756e6974733d33300a6865696768745f756e6974733d32300a7461726765745f617263683d7838365f36342d756e6b6e",
    "6f776e2d6e6f6e650a7075626c69736865725f6b65795f69643d656432353531393a6561346136633633653239633532",
    "306162656635353037623133326563356639393534373736616562656265376239323432316565613639313434366432",
    "32630a7061796c6f61645f686173683d3331363339343864353436386135306361386338393966313738353533363837",
    "64336631363332386132376638643661303335653363356165376631663865390a6361706162696c69746965733d6669",
    "6c6573797374656d2e757365722e726561642c6e6574776f726b2e636c69656e740a7369673d36306634333263326330",
    "373033333531356331653335633337396264623033303063643335613064336130623433613136633339613931393739",
    "633362623562303462643163316331343161306263633630616664653532326631366463343233303939656135313131",
    "366136623366323635623230346462353362663230660a4e43424d2d746573742d7061796c6f61642d62797465732d30",
    "313233343536373839",
);
const SDK_FIXTURE_PUBKEY: &str = "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c";

#[test]
fn package_built_by_python_sdk_verifies() {
    let bytes = unhex(SDK_FIXTURE_HEX);
    let t = vec![String::from(SDK_FIXTURE_PUBKEY)];
    let pkg = parse_with(&bytes, &t, &[]).expect("SDK paketi doğrulanmalı");
    assert!(pkg.is_verified);
    assert_eq!(pkg.manifest.id, "nc.user.fixture");
    assert_eq!(pkg.manifest.name, "Fixture");
    assert_eq!(pkg.manifest.width_units, 30);
    assert_eq!(pkg.manifest.capabilities.len(), 2);
    assert_eq!(pkg.payload, b"NCBM-test-payload-bytes-0123456789");
}

#[test]
fn python_sdk_package_with_flipped_signature_bit_is_rejected() {
    let mut bytes = unhex(SDK_FIXTURE_HEX);
    let i = find(&bytes, b"sig=") + 4;
    bytes[i] = if bytes[i] == b'0' { b'1' } else { b'0' };
    let t = vec![String::from(SDK_FIXTURE_PUBKEY)];
    assert_eq!(parse_with(&bytes, &t, &[]).unwrap_err(), PackageError::BadSignature);
}

// ---- kütüphane kablolaması: RFC 8032 §7.1 TEST 1 -------------------------------------------

#[test]
fn rfc8032_test_vector_1() {
    let pk = unhex("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a");
    let sig = unhex("e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b");
    let mut pk_arr = [0u8; 32];
    pk_arr.copy_from_slice(&pk);
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig);
    let vk = VerifyingKey::from_bytes(&pk_arr).unwrap();
    assert!(vk.verify_strict(b"", &Signature::from_bytes(&sig_arr)).is_ok());
    assert!(vk.verify_strict(b"x", &Signature::from_bytes(&sig_arr)).is_err());
}

// ---- F-11: SHA-256 NIST vektörleri ----------------------------------------------------------

#[test]
fn sha256_known_answers() {
    assert_eq!(hex(&compute_sha256(b"")), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    assert_eq!(hex(&compute_sha256(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    assert_eq!(
        hex(&compute_sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
}

// ---- F-05: yetki bitleri çekirdekle ortak -------------------------------------------------

#[test]
fn capability_bits_match_shared_nc_caps_definition() {
    assert_eq!(Capability::SettingsRead.bit(), nc_caps::SETTINGS_READ);
    assert_eq!(Capability::SettingsWriteNetwork.bit(), nc_caps::SETTINGS_WRITE_NETWORK);
    assert_eq!(Capability::FsUserRead.bit(), nc_caps::FS_USER_READ);
    assert_eq!(Capability::FsUserWrite.bit(), nc_caps::FS_USER_WRITE);
    assert_eq!(Capability::PackageInstall.bit(), nc_caps::PACKAGE_INSTALL);
    assert_eq!(Capability::NetworkClient.bit(), nc_caps::NETWORK_CLIENT);
    // Okuma yetkisi yazma yetkisiyle karışmamalı (eski hata: çekirdekte 1<<6 = YAZMA idi).
    assert_ne!(Capability::FsUserRead.bit(), nc_caps::FS_USER_WRITE);
}


// ---- Ring 3 zorunluluğu -------------------------------------------------------------------

const RING3_ELF: &[u8] = include_bytes!("../../../kernel/faz6/faz6_app.elf");

fn ring3_text(publisher_pub_hex: &str, payload_hash_hex: &str, abi_line: &str) -> String {
    format!(
        "id=nc.user.r3\nname=R3\nversion=1.0.0\nversion_code=1\nicon=generic\n{}\
         width_units=30\nheight_units=20\npublisher_key_id=ed25519:{}\npayload_hash={}\n\
         capabilities=fs.user.read, network.client, bogus.name\n",
        abi_line, publisher_pub_hex, payload_hash_hex
    )
}

fn build_ring3(signer: &SigningKey, abi_line: &str, payload: &[u8]) -> Vec<u8> {
    let h = compute_sha256(payload);
    let text = ring3_text(&pub_hex(signer), &hex(&h), abi_line);
    let sig = signer.sign(text.as_bytes());
    assemble(&format!("{}sig={}\n", text, hex(&sig.to_bytes())), payload, &h)
}

#[test]
fn ring3_package_with_elf_payload_is_runnable() {
    let sk = key(9);
    let bytes = build_ring3(&sk, "abi=ring3-v1\n", RING3_ELF);
    let pkg = parse_with(&bytes, &trusted(&sk), &[]).unwrap();
    assert_eq!(pkg.manifest.abi, PackageAbi::Ring3V1);
    assert!(pkg.runnable());
    assert_eq!(
        pkg.capability_bits(),
        nc_caps::FS_USER_READ | nc_caps::NETWORK_CLIENT,
        "bilinmeyen yetki adi yok sayilmali, bilinenler eslenmeli"
    );
}

#[test]
fn packages_without_ring3_abi_or_elf_are_never_runnable() {
    let sk = key(10);
    // abi yok (eski/yerel kod beklentisi) + ELF bile olsa çalıştırılamaz.
    let legacy = parse_with(&build_ring3(&sk, "", RING3_ELF), &trusted(&sk), &[]).unwrap();
    assert_eq!(legacy.manifest.abi, PackageAbi::Legacy);
    assert!(!legacy.runnable());
    // Bilinmeyen abi değeri.
    let odd = parse_with(&build_ring3(&sk, "abi=native-x86\n", RING3_ELF), &trusted(&sk), &[]).unwrap();
    assert!(!odd.runnable());
    // abi=ring3-v1 ama yük ELF değil.
    let junk = parse_with(&build_ring3(&sk, "abi=ring3-v1\n", PAYLOAD), &trusted(&sk), &[]).unwrap();
    assert!(!junk.runnable());
    // abi=ring3-v1 ama W+X bir ELF (ilk PT_LOAD bayrağı 5 -> 7).
    let mut wx = RING3_ELF.to_vec();
    wx[68] = 7;
    let bad = parse_with(&build_ring3(&sk, "abi=ring3-v1\n", &wx), &trusted(&sk), &[]).unwrap();
    assert!(!bad.runnable(), "W+X ELF çalıştırılamaz");
    // İmzasız v1 paket hiçbir koşulda çalıştırılamaz (manifestine abi yazsa bile).
    let v1_manifest = "id=nc.user.v1\nname=V1\nabi=ring3-v1\n";
    let mut v1 = Vec::new();
    v1.extend_from_slice(b"NTZC");
    for v in [1u32, 0, 0, v1_manifest.len() as u32, (28 + v1_manifest.len()) as u32, RING3_ELF.len() as u32] {
        v1.extend_from_slice(&v.to_le_bytes());
    }
    v1.extend_from_slice(v1_manifest.as_bytes());
    v1.extend_from_slice(RING3_ELF);
    let unsigned = NtzcPackage::parse_checked(&v1).unwrap();
    assert!(!unsigned.is_verified && !unsigned.runnable());
}

struct Dummy(nc_app::AppInfo);

impl nc_app::App for Dummy {
    fn info(&self) -> &nc_app::AppInfo {
        &self.0
    }
    fn update(&mut self, _ctx: &mut nc_app::Context, _input: &nc_app::Input) -> nc_app::Update {
        nc_app::Update::NONE
    }
    fn draw(&self, _canvas: &mut nc_gfx::Canvas<'_>, _area: nc_gfx::Rect, _ctx: &nc_app::Context) {}
}

#[test]
fn loader_hook_only_runs_for_runnable_packages() {
    fn loader(pkg: &NtzcPackage) -> Option<Box<dyn nc_app::App>> {
        Some(Box::new(Dummy(nc_app::AppInfo::new(&pkg.manifest.id, "Dummy", "1", nc_app::Icon::Generic, 10, 10))))
    }
    set_package_loader(loader);
    let sk = key(11);
    let good = parse_with(&build_ring3(&sk, "abi=ring3-v1\n", RING3_ELF), &trusted(&sk), &[]).unwrap();
    assert!(load_packaged_app(&good).is_some());
    let bad = parse_with(&build_ring3(&sk, "", RING3_ELF), &trusted(&sk), &[]).unwrap();
    assert!(load_packaged_app(&bad).is_none(), "abi'siz paket kanca ile bile yuklenmemeli");
}
