//! Annotation metadata values: `/NM` identifiers and PDF date strings.

use std::time::{SystemTime, UNIX_EPOCH};

/// A fresh `/NM` value (random UUID).
pub fn new_annotation_name() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// A PDF date string (PDF 32000-1 section 7.9.4) in UTC with an explicit time zone:
/// `D:YYYYMMDDHHmmSS+00'00'`.
pub fn pdf_date(time: SystemTime) -> String {
    let secs = match time.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
        Err(e) => -i64::try_from(e.duration().as_secs()).unwrap_or(i64::MAX),
    };
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "D:{y:04}{m:02}{d:02}{:02}{:02}{:02}+00'00'",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

pub fn pdf_date_now() -> String {
    pdf_date(SystemTime::now())
}

/// Reads a PDF date string (`D:YYYYMMDDHHmmSSOHH'mm'`, every part after the year
/// optional, the `D:` prefix too) as milliseconds since the Unix epoch. Lenient, as other
/// apps write dates loosely: a missing apostrophe or time zone is fine (no zone means UTC).
pub fn parse_pdf_date(s: &str) -> Option<i64> {
    let s = s.trim();
    let s = s.strip_prefix("D:").unwrap_or(s);
    let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
    if digits.len() < 4 {
        return None;
    }
    let part = |from: usize, len: usize, default: i64| -> Option<i64> {
        match digits.get(from..from + len) {
            Some(p) => p.parse().ok(),
            None => Some(default),
        }
    };
    let year = part(0, 4, 0)?;
    let month = part(4, 2, 1)?;
    let day = part(6, 2, 1)?;
    let hour = part(8, 2, 0)?;
    let minute = part(10, 2, 0)?;
    let second = part(12, 2, 0)?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let rest = &s[digits.len()..];
    let offset_minutes = match rest.chars().next() {
        Some(sign @ ('+' | '-')) => {
            let zone: String = rest[1..].chars().filter(char::is_ascii_digit).collect();
            let h: i64 = zone.get(0..2).and_then(|v| v.parse().ok()).unwrap_or(0);
            let m: i64 = zone.get(2..4).and_then(|v| v.parse().ok()).unwrap_or(0);
            if sign == '+' {
                h * 60 + m
            } else {
                -(h * 60 + m)
            }
        }
        _ => 0,
    };
    let days = days_from_civil(year, month, day);
    let local = days * 86_400 + hour * 3600 + minute * 60 + second;
    Some((local - offset_minutes * 60) * 1000)
}

/// (year, month, day) to days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian calendar
/// (Howard Hinnant's `civil_from_days`).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    // d is in 1..=31 and m in 1..=12 by construction, so the casts are lossless.
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn formats_pdf_dates_with_time_zone() {
        assert_eq!(pdf_date(UNIX_EPOCH), "D:19700101000000+00'00'");
        // 2026-10-02 23:15:07 UTC
        let t = UNIX_EPOCH + Duration::from_secs(1_790_982_907);
        assert_eq!(pdf_date(t), "D:20261002231507+00'00'");
        // Leap day.
        let t = UNIX_EPOCH + Duration::from_secs(951_782_400);
        assert_eq!(pdf_date(t), "D:20000229000000+00'00'");
    }

    #[test]
    fn parses_dates_written_by_other_apps() {
        let t = |s| parse_pdf_date(s).map(|ms| ms / 1000);
        assert_eq!(t("D:20261002231507+00'00'"), Some(1_790_982_907));
        assert_eq!(t("D:20261002231507Z"), Some(1_790_982_907));
        assert_eq!(t("D:20261002231507"), Some(1_790_982_907));
        // Acrobat: local time with an offset.
        assert_eq!(t("D:20261003011507+02'00'"), Some(1_790_982_907));
        assert_eq!(t("D:20261002181507-05'00"), Some(1_790_982_907));
        assert_eq!(t("20261002231507+00'00'"), Some(1_790_982_907));
        assert_eq!(t("D:2026"), t("D:20260101000000Z"));
        assert_eq!(t("D:1999"), Some(915_148_800));
        assert_eq!(t("yesterday"), None);
        assert_eq!(t("D:20261302"), None);
        // Our own dates read back.
        let now = SystemTime::now();
        let secs = now.duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        assert_eq!(t(&pdf_date(now)), Some(secs));
    }

    #[test]
    fn names_are_unique_uuids() {
        let a = new_annotation_name();
        let b = new_annotation_name();
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
    }
}
