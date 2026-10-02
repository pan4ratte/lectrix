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
    fn names_are_unique_uuids() {
        let a = new_annotation_name();
        let b = new_annotation_name();
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
    }
}
