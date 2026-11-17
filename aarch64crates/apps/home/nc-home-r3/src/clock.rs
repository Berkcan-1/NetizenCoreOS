#![allow(dead_code)]
pub const TZ_SECONDS: i64 = 3 * 3600;

pub const DAYS: [&str; 7] = ["Pazar", "Pazartesi", "Salı", "Çarşamba", "Perşembe", "Cuma", "Cumartesi"];
pub const MONTHS: [&str; 12] = [
    "Ocak", "Şubat", "Mart", "Nisan", "Mayıs", "Haziran", "Temmuz", "Ağustos", "Eylül", "Ekim", "Kasım", "Aralık",
];

#[derive(Clone, Copy)]
pub struct Local {
    pub hour: u8,
    pub minute: u8,
    pub weekday: u8,
    pub day: u8,
    pub month: u8,
    pub year: i32,
}

pub fn from_unix(utc: i64) -> Option<Local> {
    from_unix_tz(utc, 3)
}

pub fn from_unix_tz(utc: i64, tz_hours: i32) -> Option<Local> {
    if utc <= 0 {
        return None;
    }
    let local = utc + (tz_hours as i64) * 3600;
    let days = local.div_euclid(86_400);
    let rest = local.rem_euclid(86_400);
    let weekday = (days + 4).rem_euclid(7) as u8;

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u8;
    let year = (year + if month <= 2 { 1 } else { 0 }) as i32;
    Some(Local {
        hour: (rest / 3600) as u8,
        minute: ((rest % 3600) / 60) as u8,
        weekday,
        day,
        month,
        year,
    })
}

pub fn format_time(local: &Local, out: &mut [u8; 5]) {
    out[0] = b'0' + local.hour / 10;
    out[1] = b'0' + local.hour % 10;
    out[2] = b':';
    out[3] = b'0' + local.minute / 10;
    out[4] = b'0' + local.minute % 10;
}
