//! Thin Rust adapter over the authoritative SPARK Ada filesystem policy (`NC_Security_Filesystem`).
//! No independent Rust path-validation or permission-evaluation engine remains here.
#![no_std]

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

pub fn check(app_id: &str, caps: u32, path: &str, op: Op) -> Result<(), Denied> {
    let op_code = match op {
        Op::Read => 0u32,
        Op::Write => 1u32,
        Op::Delete => 2u32,
    };
    match nc_spark_sec::fs_check(app_id, caps as u64, path, op_code) {
        nc_spark_sec::FS_ALLOW => Ok(()),
        nc_spark_sec::FS_DENY_BAD_PATH => Err(Denied::BadPath),
        nc_spark_sec::FS_DENY_READ_ONLY => Err(Denied::ReadOnly),
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

    #[test]
    fn usb_storage_policy_enforces_capabilities() {
        assert_eq!(check("demo", NONE, "/storage", Op::Read), Err(Denied::Permission));
        assert_eq!(check("demo", FS_USER_READ, "/storage", Op::Read), Ok(()));
        assert_eq!(check("demo", FS_USER_READ, "/storage", Op::Write), Err(Denied::ReadOnly));
        assert_eq!(check("demo", FS_USER_READ, "/storage/usb0/app.ntzc", Op::Read), Ok(()));
        assert_eq!(check("demo", FS_USER_READ, "/storage/usb0/out.txt", Op::Write), Err(Denied::Permission));
        assert_eq!(check("nc.files", SYSTEM, "/storage/usb0/app.ntzc", Op::Read), Ok(()));
    }

    #[test]
    fn kernel_security_and_contacts_db_are_inaccessible_to_all_ring3_apps() {
        assert_eq!(
            check("nc.files", SYSTEM, "/data/appdata/nc.security/permissions.xml", Op::Read),
            Err(Denied::Permission)
        );
        assert_eq!(
            check("nc.files", SYSTEM, "/data/appdata/nc.security/permissions.xml", Op::Write),
            Err(Denied::Permission)
        );
        assert_eq!(
            check("nc.settings", SYSTEM, "/data/appdata/nc.security/permissions.xml", Op::Write),
            Err(Denied::Permission)
        );
        assert_eq!(
            check("nc.contacts", SYSTEM, "/data/appdata/nc.contacts.db/contacts.xml", Op::Read),
            Err(Denied::Permission)
        );
        assert_eq!(
            check("nc.contacts", USER, "/data/appdata/nc.contacts/prefs.cfg", Op::Write),
            Ok(())
        );
    }
}
