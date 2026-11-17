#![no_std]

use nc_caps::{FS_ALL_READ, FS_ALL_WRITE, FS_USER_DELETE, FS_USER_READ, FS_USER_WRITE};

pub const MAX_DEPTH: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Read,
    Write,
    Delete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Denied {
    BadPath,
    ReadOnly,
    Permission,
}

fn split<'a>(path: &'a str, parts: &mut [&'a str; MAX_DEPTH]) -> Option<usize> {
    if !path.starts_with('/') || path.bytes().any(|b| b == 0 || b == b'\\') {
        return None;
    }
    let mut count = 0usize;
    let body = &path[1..];
    if body.is_empty() {
        return Some(0);
    }
    let body = body.strip_suffix('/').unwrap_or(body);
    for part in body.split('/') {
        if part.is_empty() || part == "." || part == ".." || count >= MAX_DEPTH {
            return None;
        }
        parts[count] = part;
        count += 1;
    }
    Some(count)
}

fn need(caps: u32, bit: u32) -> Result<(), Denied> {
    if caps & bit != 0 {
        Ok(())
    } else {
        Err(Denied::Permission)
    }
}

fn need_any(caps: u32, first: u32, second: u32) -> Result<(), Denied> {
    if caps & (first | second) != 0 {
        Ok(())
    } else {
        Err(Denied::Permission)
    }
}

fn all_access(caps: u32, op: Op) -> Result<(), Denied> {
    match op {
        Op::Read => need(caps, FS_ALL_READ),
        Op::Write | Op::Delete => need(caps, FS_ALL_WRITE),
    }
}

pub fn check(app_id: &str, caps: u32, path: &str, op: Op) -> Result<(), Denied> {
    let mut storage = [""; MAX_DEPTH];
    let count = split(path, &mut storage).ok_or(Denied::BadPath)?;
    let parts = &storage[..count];
    match parts {
        [] => {
            if op == Op::Read {
                Ok(())
            } else {
                Err(Denied::ReadOnly)
            }
        }
        ["system", ..] => {
            if op != Op::Read {
                Err(Denied::ReadOnly)
            } else {
                need(caps, FS_ALL_READ)
            }
        }
        ["data"] => {
            if op == Op::Read {
                Ok(())
            } else {
                Err(Denied::Permission)
            }
        }
        ["data", "app", ..] => all_access(caps, op),
        ["data", "appdata"] => {
            if op == Op::Read {
                need(caps, FS_ALL_READ)
            } else {
                Err(Denied::Permission)
            }
        }
        ["data", "appdata", owner, ..] => {
            if !app_id.is_empty() && *owner == app_id {
                Ok(())
            } else {
                all_access(caps, op)
            }
        }
        ["data", "media", ..] => match op {
            Op::Read => need_any(caps, FS_USER_READ, FS_ALL_READ),
            Op::Write => need_any(caps, FS_USER_WRITE, FS_ALL_WRITE),
            Op::Delete => need_any(caps, FS_USER_DELETE, FS_ALL_WRITE),
        },
        _ => Err(Denied::Permission),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nc_caps::*;

    const USER: u32 = USER_ALLOWED;
    const SYSTEM: u32 = FS_ALL_READ | FS_ALL_WRITE | PACKAGE_INSTALL | PROCESS_LAUNCH;

    #[test]
    fn traversal_and_malformed_paths_are_rejected() {
        for path in ["", "data", "/data/../system", "/data//app", "/data/./app", "/a\\b", "/data/app/\0x"] {
            assert_eq!(check("x", SYSTEM, path, Op::Read), Err(Denied::BadPath), "{path}");
        }
    }

    #[test]
    fn system_partition_is_never_writable() {
        assert_eq!(check("nc.files", SYSTEM, "/system/etc/x", Op::Write), Err(Denied::ReadOnly));
        assert_eq!(check("nc.files", SYSTEM, "/system/packages", Op::Delete), Err(Denied::ReadOnly));
        assert_eq!(check("nc.files", SYSTEM, "/system/packages", Op::Read), Ok(()));
        assert_eq!(check("demo", USER, "/system/packages", Op::Read), Err(Denied::Permission));
    }

    #[test]
    fn user_app_owns_only_its_own_sandbox() {
        assert_eq!(check("demo.a", USER, "/data/appdata/demo.a/save.bin", Op::Write), Ok(()));
        assert_eq!(check("demo.a", USER, "/data/appdata/demo.a", Op::Delete), Ok(()));
        assert_eq!(check("demo.a", USER, "/data/appdata/demo.b/save.bin", Op::Read), Err(Denied::Permission));
        assert_eq!(check("demo.a", USER, "/data/appdata", Op::Read), Err(Denied::Permission));
        assert_eq!(check("demo.a", USER, "/data/app/demo.b.ntzc", Op::Read), Err(Denied::Permission));
        assert_eq!(check("demo.a", USER, "/data/app/demo.a.ntzc", Op::Write), Err(Denied::Permission));
    }

    #[test]
    fn prefix_of_another_app_id_is_not_ownership() {
        assert_eq!(check("demo", USER, "/data/appdata/demo.a/x", Op::Read), Err(Denied::Permission));
        assert_eq!(check("", USER, "/data/appdata//x", Op::Read), Err(Denied::BadPath));
        assert_eq!(check("", USER, "/data/appdata/x/y", Op::Read), Err(Denied::Permission));
    }

    #[test]
    fn shared_media_needs_the_user_storage_caps() {
        assert_eq!(check("demo", NONE, "/data/media/a.png", Op::Read), Err(Denied::Permission));
        assert_eq!(check("demo", FS_USER_READ, "/data/media/a.png", Op::Read), Ok(()));
        assert_eq!(check("demo", FS_USER_READ, "/data/media/a.png", Op::Write), Err(Denied::Permission));
        assert_eq!(check("demo", FS_USER_WRITE, "/data/media/a.png", Op::Write), Ok(()));
        assert_eq!(check("demo", FS_USER_WRITE, "/data/media/a.png", Op::Delete), Err(Denied::Permission));
        assert_eq!(check("demo", FS_USER_DELETE, "/data/media/a.png", Op::Delete), Ok(()));
    }

    #[test]
    fn system_tier_sees_everything_under_data_but_roots_stay_fixed() {
        assert_eq!(check("nc.files", SYSTEM, "/data/appdata/demo.a/x", Op::Read), Ok(()));
        assert_eq!(check("nc.files", SYSTEM, "/data/app/demo.a.ntzc", Op::Delete), Ok(()));
        assert_eq!(check("nc.files", SYSTEM, "/data/media/x", Op::Write), Ok(()));
        assert_eq!(check("nc.files", SYSTEM, "/data", Op::Write), Err(Denied::Permission));
        assert_eq!(check("nc.files", SYSTEM, "/", Op::Write), Err(Denied::ReadOnly));
        assert_eq!(check("nc.files", SYSTEM, "/nope", Op::Read), Err(Denied::Permission));
    }

    #[test]
    fn user_tier_caps_from_manifest_cannot_reach_all_access() {
        let declared = FS_ALL_READ | FS_ALL_WRITE | FS_USER_READ;
        let granted = clamp_user(declared);
        assert_eq!(check("demo", granted, "/data/app/x.ntzc", Op::Read), Err(Denied::Permission));
        assert_eq!(check("demo", granted, "/data/media/x", Op::Read), Ok(()));
    }
}
