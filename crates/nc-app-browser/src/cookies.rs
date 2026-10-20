//! Minimal cookie jar shared by page loads and the JavaScript network pump.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone)]
struct Cookie {
    domain: String,
    host_only: bool,
    path: String,
    name: String,
    value: String,
    secure: bool,
    http_only: bool,
    /// Jar-clock deadline in milliseconds. `None` = session cookie.
    expires_at: Option<f64>,
}

pub struct CookieJar {
    cookies: Vec<Cookie>,
    /// Monotonic jar clock in milliseconds, advanced by the browser frame loop.
    now_ms: f64,
}

/// Multi-label public suffixes that must never be used as a cookie `Domain`.
/// Not the full Public Suffix List, only the commonly abused entries.
const PUBLIC_SUFFIXES: &[&str] = &[
    "co.uk", "org.uk", "ac.uk", "gov.uk", "me.uk", "com.tr", "org.tr", "net.tr", "gov.tr", "edu.tr", "k12.tr",
    "com.au", "net.au", "org.au", "co.jp", "ne.jp", "or.jp", "co.in", "com.br", "co.za", "co.nz", "com.cn", "com.mx",
    "github.io", "gitlab.io", "herokuapp.com", "blogspot.com", "pages.dev", "netlify.app", "vercel.app", "web.app",
    "firebaseapp.com", "appspot.com", "azurewebsites.net", "cloudfront.net",
];

fn is_ip_literal(host: &str) -> bool {
    host.contains(':') || (!host.is_empty() && host.bytes().all(|b| b.is_ascii_digit() || b == b'.'))
}

/// RFC 6265 5.3 step 6: the `Domain` attribute is acceptable only if the request host
/// equals it or is a real subdomain of it (a `.` boundary is required), and it is not a public suffix.
fn domain_attr_acceptable(host: &str, d: &str) -> bool {
    if d.is_empty() {
        return false;
    }
    if host == d {
        return true;
    }
    if is_ip_literal(host) || !d.contains('.') || PUBLIC_SUFFIXES.iter().any(|p| *p == d) {
        return false;
    }
    host.len() > d.len() && host.ends_with(d) && host.as_bytes()[host.len() - d.len() - 1] == b'.'
}

/// Heuristic for `Expires`: the OS has no wall clock here, so only dates whose 4-digit year is
/// clearly in the past (the usual "delete this cookie" idiom, e.g. 1970) are honoured.
/// Later years are treated as "still valid" (the cookie stays a session-lifetime cookie).
fn expires_is_in_past(v: &str) -> bool {
    v.split(|c: char| !c.is_ascii_digit())
        .find(|t| t.len() == 4)
        .and_then(|t| t.parse::<u32>().ok())
        .map(|y| y < 2026)
        .unwrap_or(false)
}

/// (is_https, host, path) of an absolute URL.
fn split_url(url: &str) -> (bool, String, String) {
    let lower = url.to_ascii_lowercase();
    let https = lower.starts_with("https://");
    let rest = if https {
        &url[8..]
    } else if lower.starts_with("http://") {
        &url[7..]
    } else {
        url
    };
    let end = rest.find('/').unwrap_or(rest.len());
    let authority = &rest[..end];
    let host = match authority.rfind(':') {
        Some(c) if authority[c + 1..].chars().all(|d| d.is_ascii_digit()) => &authority[..c],
        _ => authority,
    };
    let path_full = if end < rest.len() { &rest[end..] } else { "/" };
    let path_end = path_full.find(|c| c == '?' || c == '#').unwrap_or(path_full.len());
    let path = &path_full[..path_end];
    (https, host.to_ascii_lowercase(), String::from(if path.is_empty() { "/" } else { path }))
}

fn domain_matches(c: &Cookie, host: &str) -> bool {
    if c.host_only {
        host == c.domain
    } else {
        host == c.domain || (host.len() > c.domain.len() && host.ends_with(c.domain.as_str()) && host.as_bytes()[host.len() - c.domain.len() - 1] == b'.')
    }
}

fn path_matches(cookie_path: &str, req_path: &str) -> bool {
    if cookie_path == "/" || req_path == cookie_path {
        return true;
    }
    req_path.starts_with(cookie_path) && (cookie_path.ends_with('/') || req_path.as_bytes().get(cookie_path.len()) == Some(&b'/'))
}

impl CookieJar {
    pub fn new() -> CookieJar {
        CookieJar { cookies: Vec::new(), now_ms: 0.0 }
    }

    /// Advances the jar clock; expired cookies are dropped.
    pub fn tick(&mut self, dt_ms: f64) {
        self.now_ms += dt_ms;
        let now = self.now_ms;
        self.cookies.retain(|c| c.expires_at.map(|t| t > now).unwrap_or(true));
    }

    fn put(&mut self, c: Cookie, remove: bool) {
        self.cookies.retain(|o| !(o.name == c.name && o.domain == c.domain && o.path == c.path));
        if !remove && self.cookies.len() < 300 {
            self.cookies.push(c);
        }
    }

    /// Stores a `Set-Cookie` value received from `host`. `from_script` cookies cannot be HttpOnly.
    pub fn store(&mut self, set_cookie: &str, host: &str, default_path: &str, from_script: bool) {
        let mut parts = set_cookie.split(';');
        let first = match parts.next() {
            Some(f) => f.trim(),
            None => return,
        };
        let (name, value) = match first.split_once('=') {
            Some((n, v)) => (n.trim(), v.trim()),
            None => return,
        };
        if name.is_empty() {
            return;
        }
        let host = host.to_ascii_lowercase();
        let mut cookie = Cookie {
            domain: host.clone(),
            host_only: true,
            path: String::from(default_path),
            name: String::from(name),
            value: String::from(value),
            secure: false,
            http_only: false,
            expires_at: None,
        };
        let mut remove = false;
        let mut max_age_seen = false;
        for attr in parts {
            let attr = attr.trim();
            let (k, v) = match attr.split_once('=') {
                Some((k, v)) => (k.trim().to_ascii_lowercase(), v.trim()),
                None => (attr.to_ascii_lowercase(), ""),
            };
            match k.as_str() {
                "domain" => {
                    let d = v.trim_start_matches('.').to_ascii_lowercase();
                    if d.is_empty() {
                        // empty Domain attribute is ignored (host-only cookie)
                    } else if domain_attr_acceptable(&host, &d) {
                        if d != host {
                            cookie.domain = d;
                            cookie.host_only = false;
                        }
                    } else {
                        // RFC 6265: a cookie whose Domain does not domain-match the request host is rejected.
                        return;
                    }
                }
                "path" => {
                    if v.starts_with('/') {
                        cookie.path = String::from(v);
                    }
                }
                "max-age" => {
                    if let Ok(n) = v.parse::<i64>() {
                        max_age_seen = true;
                        if n <= 0 {
                            remove = true;
                        } else {
                            remove = false;
                            cookie.expires_at = Some(self.now_ms + (n as f64) * 1000.0);
                        }
                    }
                }
                "expires" => {
                    // Max-Age takes precedence over Expires (RFC 6265 4.1.2.2).
                    if !max_age_seen && expires_is_in_past(v) {
                        remove = true;
                    }
                }
                "secure" => cookie.secure = true,
                "httponly" => cookie.http_only = true,
                _ => {}
            }
        }
        if from_script {
            cookie.http_only = false;
        }
        self.put(cookie, remove);
    }

    /// `Cookie` request header value for `url`.
    pub fn header_for(&self, url: &str) -> String {
        self.collect(url, true)
    }

    /// `document.cookie` value for `url` (HttpOnly cookies hidden).
    pub fn script_visible(&self, url: &str) -> String {
        self.collect(url, false)
    }

    fn collect(&self, url: &str, include_http_only: bool) -> String {
        let (https, host, path) = split_url(url);
        let mut out = String::new();
        for c in self.cookies.iter() {
            if (c.http_only && !include_http_only) || (c.secure && !https) || !domain_matches(c, &host) || !path_matches(&c.path, &path) {
                continue;
            }
            if !out.is_empty() {
                out.push_str("; ");
            }
            out.push_str(&c.name);
            out.push('=');
            out.push_str(&c.value);
        }
        out
    }

    pub fn set_from_script(&mut self, text: &str, url: &str) {
        let (_, host, path) = split_url(url);
        let dir = match path.rfind('/') {
            Some(0) | None => String::from("/"),
            Some(i) => String::from(&path[..i]),
        };
        self.store(text, &host, &dir, true);
    }

    pub fn store_from_response(&mut self, set_cookie: &str, host: &str) {
        self.store(set_cookie, host, "/", false);
    }
}
