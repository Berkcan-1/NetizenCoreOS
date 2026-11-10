// crates/nc-arch/src/rtc.rs
//
// F-04: CMOS gerçek zaman saati (RTC) okuyucu. TLS sertifika geçerlilik süresi kontrolü
// (`not_before` / `not_after`) için Unix zamanı sağlar.
//
// DURUM: Yazıldığı ortamda derleme/çalıştırma imkânı yoktu. Saf çözümleme mantığı
// (`decode_rtc`, `civil_to_unix`) birim testlerle kapsanır; port G/Ç kısmı (`read_cmos_time`)
// yalnızca QEMU/VirtualBox/gerçek donanımda doğrulanabilir (seri günlükte tarihi kontrol et).
use crate::{inb, outb};

const CMOS_ADDR: u16 = 0x70;
const CMOS_DATA: u16 = 0x71;

const REG_SECONDS: u8 = 0x00;
const REG_MINUTES: u8 = 0x02;
const REG_HOURS: u8 = 0x04;
const REG_DAY: u8 = 0x07;
const REG_MONTH: u8 = 0x08;
const REG_YEAR: u8 = 0x09;
const REG_STATUS_A: u8 = 0x0A;
const REG_STATUS_B: u8 = 0x0B;
/// ACPI FADT "century" yazmacı çoğu PC'de/VM'de 0x32'dir (garanti değildir; değer doğrulanır).
const REG_CENTURY: u8 = 0x32;

const STATUS_A_UPDATE_IN_PROGRESS: u8 = 0x80;
const STATUS_B_24_HOUR: u8 = 0x02;
const STATUS_B_BINARY: u8 = 0x04;

/// 2024-01-01T00:00:00Z. Bundan önceki bir saat "ayarlanmamış/pil bitmiş" sayılır ve
/// bilinmeyen (None) döndürülür; böylece bozuk saatle sertifika kabul/ret kararı verilmez.
pub const MIN_PLAUSIBLE_UNIX: u64 = 1_704_067_200;

/// Ham CMOS değerleri (çözümlenmemiş).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RawRtc {
    pub second: u8,
    pub minute: u8,
    pub hour: u8,
    pub day: u8,
    pub month: u8,
    pub year: u8,
    pub century: u8,
}

#[inline]
unsafe fn cmos_read(reg: u8) -> u8 {
    outb(CMOS_ADDR, reg);
    inb(CMOS_DATA)
}

fn bcd_to_bin(v: u8) -> Option<u8> {
    let (hi, lo) = (v >> 4, v & 0x0F);
    if hi > 9 || lo > 9 {
        return None;
    }
    Some(hi * 10 + lo)
}

/// Gregoryen takvim -> 1970-01-01'den itibaren gün sayısı (Howard Hinnant algoritması).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// Takvim alanlarından Unix saniyesine. Geçersiz alan -> None.
pub fn civil_to_unix(year: i64, month: i64, day: i64, hour: i64, min: i64, sec: i64) -> Option<u64> {
    if !(1970..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || day < 1
        || day > days_in_month(year, month)
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&min)
        || !(0..=59).contains(&sec)
    {
        return None;
    }
    let secs = days_from_civil(year, month, day) * 86_400 + hour * 3600 + min * 60 + sec;
    if secs < 0 {
        None
    } else {
        Some(secs as u64)
    }
}

/// Ham RTC değerlerini (BCD/ikili, 12/24 saat) Unix saniyesine çevirir. Saf fonksiyon.
pub fn decode_rtc(raw: RawRtc, status_b: u8) -> Option<u64> {
    let binary = status_b & STATUS_B_BINARY != 0;
    let hour_24 = status_b & STATUS_B_24_HOUR != 0;

    let conv = |v: u8| -> Option<u8> {
        if binary {
            Some(v)
        } else {
            bcd_to_bin(v)
        }
    };

    // 12 saat kipinde PM biti saat baytının 7. bitidir; dönüşümden ÖNCE ayrılmalıdır.
    let pm = !hour_24 && (raw.hour & 0x80) != 0;
    let hour_raw = if hour_24 { raw.hour } else { raw.hour & 0x7F };

    let second = conv(raw.second)?;
    let minute = conv(raw.minute)?;
    let mut hour = conv(hour_raw)?;
    let day = conv(raw.day)?;
    let month = conv(raw.month)?;
    let yy = conv(raw.year)?;

    if !hour_24 {
        // 12 saat: 1..=12; 12 AM -> 0, 12 PM -> 12, diğer PM -> +12
        if hour == 0 || hour > 12 {
            return None;
        }
        hour %= 12;
        if pm {
            hour += 12;
        }
    }

    // Yüzyıl yazmacı yalnızca makul değerlerde (20 veya 21) kullanılır; aksi halde 2000'ler varsayılır.
    let century = conv(raw.century).filter(|c| *c == 20 || *c == 21).unwrap_or(20);
    let year = century as i64 * 100 + yy as i64;

    let unix = civil_to_unix(
        year,
        month as i64,
        day as i64,
        hour as i64,
        minute as i64,
        second as i64,
    )?;
    if unix < MIN_PLAUSIBLE_UNIX {
        return None;
    }
    Some(unix)
}

fn read_raw() -> RawRtc {
    unsafe {
        RawRtc {
            second: cmos_read(REG_SECONDS),
            minute: cmos_read(REG_MINUTES),
            hour: cmos_read(REG_HOURS),
            day: cmos_read(REG_DAY),
            month: cmos_read(REG_MONTH),
            year: cmos_read(REG_YEAR),
            century: cmos_read(REG_CENTURY),
        }
    }
}

/// "Güncelleme sürüyor" bayrağı temizlenene kadar bekler (sınırlı döngü). Başarısızsa false.
fn wait_not_updating() -> bool {
    for _ in 0..200_000u32 {
        let a = unsafe { cmos_read(REG_STATUS_A) };
        if a & STATUS_A_UPDATE_IN_PROGRESS == 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

/// CMOS saatini okur: güncelleme bayrağını bekler ve iki ardışık okuma aynı olana kadar tekrarlar
/// (okuma sırasında saniye taşması yüzünden tutarsız değer almamak için).
/// Saat okunamıyor / ayarsız / makul değilse `None`.
pub fn read_cmos_time() -> Option<u64> {
    let mut prev: Option<RawRtc> = None;
    for _ in 0..8 {
        if !wait_not_updating() {
            return None;
        }
        let now = read_raw();
        if prev == Some(now) {
            let status_b = unsafe { cmos_read(REG_STATUS_B) };
            return decode_rtc(now, status_b);
        }
        prev = Some(now);
    }
    None
}

/// Şu anki UTC zamanı (Unix saniyesi). RTC'nin UTC'de tutulduğu varsayılır (QEMU/VirtualBox
/// varsayılanı; çok önyüklemeli Windows makinelerinde RTC yerel saat olabilir -> sertifika
/// süresi kontrolü o saat dilimi kadar kayar).
pub fn unix_time_now() -> Option<u64> {
    read_cmos_time()
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-11-05T12:34:56Z
    const T_2026_11_05_123456: u64 = 1_793_882_096;

    #[test]
    fn civil_known_values() {
        assert_eq!(civil_to_unix(1970, 1, 1, 0, 0, 0), Some(0));
        assert_eq!(civil_to_unix(2000, 1, 1, 0, 0, 0), Some(946_684_800));
        assert_eq!(civil_to_unix(2024, 1, 1, 0, 0, 0), Some(MIN_PLAUSIBLE_UNIX));
        assert_eq!(civil_to_unix(2024, 2, 29, 12, 0, 0), Some(1_709_208_000)); // artık yıl
        assert_eq!(civil_to_unix(2026, 11, 5, 12, 34, 56), Some(T_2026_11_05_123456));
    }

    #[test]
    fn civil_rejects_invalid_fields() {
        assert_eq!(civil_to_unix(2025, 2, 29, 0, 0, 0), None); // artık yıl değil
        assert_eq!(civil_to_unix(2025, 13, 1, 0, 0, 0), None);
        assert_eq!(civil_to_unix(2025, 4, 31, 0, 0, 0), None);
        assert_eq!(civil_to_unix(2025, 1, 1, 24, 0, 0), None);
        assert_eq!(civil_to_unix(2025, 1, 1, 0, 60, 0), None);
        assert_eq!(civil_to_unix(2025, 1, 1, 0, 0, 60), None);
    }

    fn raw(second: u8, minute: u8, hour: u8, day: u8, month: u8, year: u8, century: u8) -> RawRtc {
        RawRtc { second, minute, hour, day, month, year, century }
    }

    #[test]
    fn decode_bcd_24h() {
        // 2026-11-05 12:34:56, BCD, 24 saat
        let r = raw(0x56, 0x34, 0x12, 0x05, 0x11, 0x26, 0x20);
        assert_eq!(decode_rtc(r, STATUS_B_24_HOUR), Some(T_2026_11_05_123456));
    }

    #[test]
    fn decode_binary_24h() {
        let r = raw(56, 34, 12, 5, 11, 26, 20);
        assert_eq!(decode_rtc(r, STATUS_B_24_HOUR | STATUS_B_BINARY), Some(T_2026_11_05_123456));
    }

    #[test]
    fn decode_bcd_12h_pm_and_midnight_noon() {
        // 12 saat kipi, BCD: 12:34:56 PM -> 12:34:56
        let pm_noon = raw(0x56, 0x34, 0x12 | 0x80, 0x05, 0x11, 0x26, 0x20);
        assert_eq!(decode_rtc(pm_noon, 0), Some(T_2026_11_05_123456));
        // 03:34:56 PM -> 15:34:56
        let pm3 = raw(0x56, 0x34, 0x03 | 0x80, 0x05, 0x11, 0x26, 0x20);
        assert_eq!(decode_rtc(pm3, 0), Some(T_2026_11_05_123456 + 3 * 3600));
        // 12:34:56 AM -> 00:34:56
        let am_midnight = raw(0x56, 0x34, 0x12, 0x05, 0x11, 0x26, 0x20);
        assert_eq!(decode_rtc(am_midnight, 0), Some(T_2026_11_05_123456 - 12 * 3600));
    }

    #[test]
    fn decode_rejects_garbage_and_unset_clock() {
        // Geçersiz BCD
        assert_eq!(decode_rtc(raw(0x5A, 0x34, 0x12, 0x05, 0x11, 0x26, 0x20), STATUS_B_24_HOUR), None);
        // Pil bitmiş/ayarsız: 2000-01-01 -> makul eşiğin altında
        assert_eq!(decode_rtc(raw(0, 0, 0, 0x01, 0x01, 0x00, 0x20), STATUS_B_24_HOUR), None);
        // Geçersiz ay/gün
        assert_eq!(decode_rtc(raw(0, 0, 0, 0x31, 0x02, 0x26, 0x20), STATUS_B_24_HOUR), None);
    }

    #[test]
    fn century_register_garbage_falls_back_to_2000s() {
        let r = raw(0x56, 0x34, 0x12, 0x05, 0x11, 0x26, 0xFF);
        assert_eq!(decode_rtc(r, STATUS_B_24_HOUR), Some(T_2026_11_05_123456));
    }
}
