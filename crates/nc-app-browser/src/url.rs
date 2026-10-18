extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

pub struct ParsedUrl {
    pub scheme: String,
    pub host: String,
    pub port: Option<u16>,
    pub path: String,
}

impl ParsedUrl {
    pub fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        let (scheme, rest) = if let Some(pos) = trimmed.find("://") {
            (&trimmed[..pos], &trimmed[pos + 3..])
        } else {
            return None;
        };

        let (authority, path) = if let Some(slash) = rest.find('/') {
            (&rest[..slash], &rest[slash..])
        } else {
            (rest, "/")
        };

        let (host, port) = if let Some(colon) = authority.rfind(':') {
            let p_str = &authority[colon + 1..];
            let mut p_val: u16 = 0;
            let mut valid = !p_str.is_empty();
            for b in p_str.bytes() {
                if b.is_ascii_digit() {
                    p_val = p_val.saturating_mul(10).saturating_add((b - b'0') as u16);
                } else {
                    valid = false;
                    break;
                }
            }
            if valid && p_val > 0 {
                (&authority[..colon], Some(p_val))
            } else {
                (authority, None)
            }
        } else {
            (authority, None)
        };

        Some(ParsedUrl {
            scheme: scheme.to_ascii_lowercase(),
            host: String::from(host),
            port,
            path: String::from(path),
        })
    }

    pub fn origin(&self) -> String {
        if let Some(p) = self.port {
            alloc::format!("{}://{}:{}", self.scheme, self.host, p)
        } else {
            alloc::format!("{}://{}", self.scheme, self.host)
        }
    }

    pub fn base_directory(&self) -> String {
        let path_no_query = if let Some(q) = self.path.find('?') {
            &self.path[..q]
        } else {
            &self.path
        };
        if let Some(slash) = path_no_query.rfind('/') {
            if slash == 0 {
                String::from("/")
            } else {
                String::from(&path_no_query[..slash + 1])
            }
        } else {
            String::from("/")
        }
    }
}

pub fn normalize_input_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::from("http://info.cern.ch/hypertext/WWW/TheProject.html");
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") || trimmed.starts_with("data:") {
        return String::from(trimmed);
    }
    if trimmed.contains('.') && !trimmed.contains(' ') {
        return alloc::format!("https://{}", trimmed);
    }
    alloc::format!("https://html.duckduckgo.com/html/?q={}", encode_uri_component(trimmed))
}

pub fn resolve_url(base: &str, href: &str) -> String {
    let h = href.trim();
    if h.is_empty() || h.starts_with('#') || h.starts_with("javascript:") {
        return String::from(base);
    }
    if h.starts_with("http://") || h.starts_with("https://") || h.starts_with("data:") {
        return String::from(h);
    }

    let parsed_base = match ParsedUrl::parse(base) {
        Some(b) => b,
        None => return String::from(h),
    };

    if h.starts_with("//") {
        return alloc::format!("{}:{}", parsed_base.scheme, h);
    }

    let origin = parsed_base.origin();

    if h.starts_with('/') {
        return alloc::format!("{}{}", origin, normalize_path(h));
    }

    if h.starts_with('?') {
        let base_path = if let Some(q) = parsed_base.path.find('?') {
            &parsed_base.path[..q]
        } else {
            &parsed_base.path
        };
        return alloc::format!("{}{}{}", origin, base_path, h);
    }

    let dir = parsed_base.base_directory();
    let combined = alloc::format!("{}{}", dir, h);
    alloc::format!("{}{}", origin, normalize_path(&combined))
}

fn normalize_path(path: &str) -> String {
    let (raw_path, query) = if let Some(q) = path.find('?') {
        (&path[..q], Some(&path[q..]))
    } else {
        (path, None)
    };

    let mut stack: Vec<&str> = Vec::new();
    for seg in raw_path.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." {
            stack.pop();
        } else {
            stack.push(seg);
        }
    }

    let mut out = String::from("/");
    for (i, seg) in stack.iter().enumerate() {
        if i > 0 {
            out.push('/');
        }
        out.push_str(seg);
    }
    if raw_path.ends_with('/') && out.len() > 1 {
        out.push('/');
    }
    if let Some(q) = query {
        out.push_str(q);
    }
    out
}

pub fn encode_uri_component(input: &str) -> String {
    let mut out = String::new();
    let hex = b"0123456789ABCDEF";
    for b in input.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            out.push(b as char);
        } else if b == b' ' {
            out.push('+');
        } else {
            out.push('%');
            out.push(hex[(b >> 4) as usize] as char);
            out.push(hex[(b & 0x0F) as usize] as char);
        }
    }
    out
}

pub fn build_form_url(base_url: &str, action: &str, param_name: &str, value: &str) -> String {
    let target = if action.trim().is_empty() {
        String::from(base_url)
    } else {
        resolve_url(base_url, action)
    };
    let sep = if target.contains('?') { '&' } else { '?' };
    let key = if param_name.trim().is_empty() { "q" } else { param_name.trim() };
    alloc::format!("{}{}{}={}", target, sep, key, encode_uri_component(value))
}
