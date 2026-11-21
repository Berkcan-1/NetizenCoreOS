//! System-managed Contacts database store and bounded, corruption-tolerant XML persistence.
//!
//! Stored under `/data/appdata/nc.contacts.db/contacts.xml` (inaccessible to Ring 3 VFS).
//! All reads/writes go through the trusted kernel Contacts service (`OP_CONTACTS_CALL`)
//! after enforcing `contacts.read` / `contacts.write` runtime permissions.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

pub const CONTACTS_SCHEMA_VERSION: u32 = 1;
pub const MAX_CONTACTS: usize = 128;
pub const MAX_NAME_LEN: usize = 48;
pub const MAX_PHONE_LEN: usize = 28;
pub const MAX_EMAIL_LEN: usize = 48;
pub const MAX_NOTE_LEN: usize = 64;
pub const MAX_CONTACTS_XML_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContactRecord {
    pub id: u32,
    pub name: String,
    pub phone: String,
    pub email: String,
    pub note: String,
}

#[derive(Clone, Debug, Default)]
pub struct ContactsStore {
    next_id: u32,
    records: Vec<ContactRecord>,
}

fn sanitize_field(input: &str, max_len: usize) -> String {
    let mut out = String::new();
    for ch in input.trim().chars() {
        if ch.is_control() || ch == '|' || ch == '\0' {
            continue;
        }
        let mut tmp = [0u8; 4];
        let enc = ch.encode_utf8(&mut tmp);
        if out.len() + enc.len() > max_len {
            break;
        }
        out.push(ch);
    }
    out
}

fn xml_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

fn xml_unescape(input: &str) -> Option<String> {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('&') {
            if let Some(r) = after.strip_prefix("amp;") {
                out.push('&');
                rest = r;
            } else if let Some(r) = after.strip_prefix("lt;") {
                out.push('<');
                rest = r;
            } else if let Some(r) = after.strip_prefix("gt;") {
                out.push('>');
                rest = r;
            } else if let Some(r) = after.strip_prefix("quot;") {
                out.push('"');
                rest = r;
            } else if let Some(r) = after.strip_prefix("apos;") {
                out.push('\'');
                rest = r;
            } else {
                return None;
            }
        } else {
            let ch = rest.chars().next()?;
            out.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
    }
    Some(out)
}

fn extract_attr<'a>(body: &'a str, key: &str) -> Option<&'a str> {
    let mut rest = body;
    while !rest.is_empty() {
        rest = rest.trim_start();
        if let Some(after) = rest.strip_prefix(key) {
            if let Some(quoted) = after.strip_prefix("=\"") {
                let end = quoted.find('"')?;
                return Some(&quoted[..end]);
            }
        }
        // Skip current key="value" token
        let eq = rest.find("=\"")?;
        let after_eq = &rest[eq + 2..];
        let end = after_eq.find('"')?;
        rest = &after_eq[end + 1..];
    }
    None
}

fn contains_ci(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let h: Vec<char> = haystack.chars().flat_map(|c| c.to_lowercase()).collect();
    let n: Vec<char> = needle.chars().flat_map(|c| c.to_lowercase()).collect();
    if n.len() > h.len() {
        return false;
    }
    (0..=h.len() - n.len()).any(|i| h[i..i + n.len()] == n[..])
}

impl ContactsStore {
    pub const fn new() -> Self {
        Self { next_id: 1, records: Vec::new() }
    }

    pub fn records(&self) -> &[ContactRecord] {
        &self.records
    }

    pub fn lookup(&self, query: &str) -> Vec<&ContactRecord> {
        let q = query.trim();
        self.records
            .iter()
            .filter(|c| contains_ci(&c.name, q) || contains_ci(&c.phone, q) || contains_ci(&c.email, q))
            .collect()
    }

    /// Create (`id == 0`) or update (`id > 0`) a contact record.
    pub fn upsert(
        &mut self,
        id: u32,
        name: &str,
        phone: &str,
        email: &str,
        note: &str,
    ) -> Result<u32, &'static str> {
        let name = sanitize_field(name, MAX_NAME_LEN);
        let phone = sanitize_field(phone, MAX_PHONE_LEN);
        let email = sanitize_field(email, MAX_EMAIL_LEN);
        let note = sanitize_field(note, MAX_NOTE_LEN);
        if name.is_empty() && phone.is_empty() {
            return Err("name or phone required");
        }
        if id != 0 {
            if let Some(existing) = self.records.iter_mut().find(|r| r.id == id) {
                existing.name = name;
                existing.phone = phone;
                existing.email = email;
                existing.note = note;
                self.sort();
                return Ok(id);
            }
            return Err("contact not found");
        }
        if self.records.len() >= MAX_CONTACTS {
            return Err("contacts capacity full");
        }
        let assigned = self.next_id.max(1);
        self.next_id = assigned.saturating_add(1);
        self.records.push(ContactRecord { id: assigned, name, phone, email, note });
        self.sort();
        Ok(assigned)
    }

    pub fn delete(&mut self, id: u32) -> bool {
        let before = self.records.len();
        self.records.retain(|r| r.id != id);
        self.records.len() != before
    }

    fn sort(&mut self) {
        self.records.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()).then(a.id.cmp(&b.id)));
    }

    pub fn to_xml(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
        let _ = writeln!(
            out,
            "<contacts schema=\"{}\" next_id=\"{}\">",
            CONTACTS_SCHEMA_VERSION,
            self.next_id.max(1)
        );
        for rec in &self.records {
            let _ = writeln!(
                out,
                "  <contact id=\"{}\" name=\"{}\" phone=\"{}\" email=\"{}\" note=\"{}\"/>",
                rec.id,
                xml_escape(&rec.name),
                xml_escape(&rec.phone),
                xml_escape(&rec.email),
                xml_escape(&rec.note),
            );
        }
        let _ = writeln!(out, "</contacts>");
        out
    }

    pub fn from_xml(text: &str) -> Result<Self, &'static str> {
        if text.len() > MAX_CONTACTS_XML_BYTES {
            return Err("contacts xml too large");
        }
        let mut store = ContactsStore::new();
        let mut saw_open = false;
        let mut saw_close = false;
        let mut max_id = 0u32;

        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with("<?xml") {
                continue;
            }
            if saw_close {
                return Err("trailing data");
            }
            if !saw_open {
                let body = line.strip_prefix("<contacts ")
                    .and_then(|s| s.strip_suffix('>'))
                    .ok_or("bad contacts root")?;
                let schema = extract_attr(body, "schema")
                    .and_then(|v| v.parse::<u32>().ok())
                    .ok_or("bad schema")?;
                if schema != CONTACTS_SCHEMA_VERSION {
                    return Err("unsupported contacts schema");
                }
                if let Some(nid) = extract_attr(body, "next_id").and_then(|v| v.parse::<u32>().ok()) {
                    store.next_id = nid.max(1);
                }
                saw_open = true;
                continue;
            }
            if line == "</contacts>" {
                saw_close = true;
                continue;
            }
            let body = line.strip_prefix("<contact ")
                .and_then(|s| s.strip_suffix("/>"))
                .ok_or("bad contact tag")?;
            let id = extract_attr(body, "id")
                .and_then(|v| v.parse::<u32>().ok())
                .filter(|&v| v > 0)
                .ok_or("bad contact id")?;
            if store.records.len() >= MAX_CONTACTS || store.records.iter().any(|r| r.id == id) {
                return Err("duplicate or overflow contact");
            }
            let name = xml_unescape(extract_attr(body, "name").unwrap_or(""))
                .map(|s| sanitize_field(&s, MAX_NAME_LEN))
                .ok_or("bad name escape")?;
            let phone = xml_unescape(extract_attr(body, "phone").unwrap_or(""))
                .map(|s| sanitize_field(&s, MAX_PHONE_LEN))
                .ok_or("bad phone escape")?;
            let email = xml_unescape(extract_attr(body, "email").unwrap_or(""))
                .map(|s| sanitize_field(&s, MAX_EMAIL_LEN))
                .ok_or("bad email escape")?;
            let note = xml_unescape(extract_attr(body, "note").unwrap_or(""))
                .map(|s| sanitize_field(&s, MAX_NOTE_LEN))
                .ok_or("bad note escape")?;
            if name.is_empty() && phone.is_empty() {
                return Err("empty contact");
            }
            max_id = max_id.max(id);
            store.records.push(ContactRecord { id, name, phone, email, note });
        }

        if !saw_open || !saw_close {
            return Err("truncated contacts xml");
        }
        if store.next_id <= max_id {
            store.next_id = max_id.saturating_add(1);
        }
        store.sort();
        Ok(store)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contacts_crud_search_and_xml_roundtrip() {
        let mut store = ContactsStore::new();
        let id1 = store.upsert(0, "Ayse & Ali", "+90 555 0101", "ayse@example.com", "Aile").unwrap();
        let id2 = store.upsert(0, "Mehmet", "+90 555 0202", "", "Is").unwrap();
        assert_ne!(id1, id2);
        assert_eq!(store.lookup("ayse").len(), 1);
        assert_eq!(store.lookup("0202").len(), 1);

        let xml = store.to_xml();
        let mut reloaded = ContactsStore::from_xml(&xml).unwrap();
        assert_eq!(reloaded.records().len(), 2);
        assert_eq!(reloaded.records()[0].name, "Ayse & Ali");

        reloaded.upsert(id2, "Mehmet Kaya", "+90 555 0202", "m@k.tr", "").unwrap();
        assert_eq!(reloaded.lookup("kaya").len(), 1);
        assert!(reloaded.delete(id1));
        assert_eq!(reloaded.records().len(), 1);
    }

    #[test]
    fn contacts_xml_rejects_corrupt_and_truncated() {
        let mut store = ContactsStore::new();
        store.upsert(0, "Test", "123", "", "").unwrap();
        let xml = store.to_xml();
        assert!(ContactsStore::from_xml(&xml[..xml.len() - 8]).is_err());
        assert!(ContactsStore::from_xml(&xml.replace("schema=\"1\"", "schema=\"7\"")).is_err());
    }
}
