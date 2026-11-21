//! Bildirimler. Yetki: manifestte `notification.post`.
//!
//! İzin kullanıcıdadır. Bir uygulama ilk kez bildirim gönderince masaüstü kullanıcıya sorar ("izin ver / verme");
//! cevap kalıcıdır. İzin beklerken gönderilen bildirimler kullanıcı onaylarsa gösterilir.
//! Kullanıcı reddettiyse `post` `NotifyError::Denied` döner; uygulama bunu bir daha denememelidir
//! (`permission()` ile önceden sorulabilir). Başlık en çok 64, gövde en çok 256 bayt (UTF-8).

use crate::shell;
use nc_abi::shell as wire;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    Ask,
    Allowed,
    Denied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotifyError {
    Denied,
    Limit,
    BadRequest,
    Failed,
}

pub fn permission() -> Result<Permission, NotifyError> {
    let reply = shell::call(wire::OP_NOTIFY_STATE, 0, 0, &[]).map_err(|_| NotifyError::Failed)?;
    if reply.status == wire::ERR_DENIED {
        return Err(NotifyError::Denied);
    }
    if reply.status != 0 {
        return Err(NotifyError::Failed);
    }
    Ok(match reply.payload().first().copied() {
        Some(wire::PERM_ALLOWED) => Permission::Allowed,
        Some(wire::PERM_DENIED) => Permission::Denied,
        _ => Permission::Ask,
    })
}

pub fn post(title: &str, body: &str, level: Level) -> Result<(), NotifyError> {
    let mut payload = [0u8; 4 + wire::MAX_TITLE + wire::MAX_BODY];
    let Some(length) = wire::encode_notify_payload(&mut payload, title, body) else {
        return Err(NotifyError::BadRequest);
    };
    let code = match level {
        Level::Info => wire::LEVEL_INFO,
        Level::Success => wire::LEVEL_SUCCESS,
        Level::Warning => wire::LEVEL_WARNING,
        Level::Error => wire::LEVEL_ERROR,
    };
    let reply = shell::call(wire::OP_NOTIFY_POST, code, 0, &payload[..length]).map_err(|_| NotifyError::Failed)?;
    match reply.status {
        0 => Ok(()),
        wire::ERR_DENIED => Err(NotifyError::Denied),
        wire::ERR_LIMIT => Err(NotifyError::Limit),
        wire::ERR_BAD_REQUEST => Err(NotifyError::BadRequest),
        _ => Err(NotifyError::Failed),
    }
}
