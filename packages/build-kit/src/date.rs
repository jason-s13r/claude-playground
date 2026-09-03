//! The one date this crate formats.

use std::time::{SystemTime, UNIX_EPOCH};

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `YYYY-MM-DD` in UTC.
///
/// Days-to-civil by hand rather than a date crate: this is the only date the
/// tool ever formats, and the arithmetic is fixed.
pub fn iso_date(epoch_secs: u64) -> String {
    let days = (epoch_secs / 86_400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_come_back_as_utc_days() {
        assert_eq!(iso_date(0), "1970-01-01");
        assert_eq!(iso_date(1_756_512_000), "2025-08-30");
        // The last second of a day still belongs to that day.
        assert_eq!(iso_date(1_756_598_399), "2025-08-30");
        assert_eq!(iso_date(1_756_598_400), "2025-08-31");
        // A leap day, which is where naive day arithmetic goes wrong.
        assert_eq!(iso_date(1_709_164_800), "2024-02-29");
        // The century rule, the other place it goes wrong.
        assert_eq!(iso_date(951_782_400), "2000-02-29");
    }
}
