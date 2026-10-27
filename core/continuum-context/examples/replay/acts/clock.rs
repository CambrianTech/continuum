//! UTC instants as the captures (Unix ms) and the logs (RFC 3339) carry them. No calendar crate:
//! the example builds with the context crate alone.

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The date of a day count: the inverse of `days_from_civil`.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

/// `2026-10-26T22:00Z`, `2026-10-26T22:00:00Z` or `2026-10-26T22:00:00.123456Z` (the `Z` may be
/// left off; the time is UTC either way) as Unix milliseconds.
pub fn parse(text: &str) -> Option<u64> {
    let text = text.trim().trim_end_matches('Z');
    let (date, time) = text.split_once('T').unwrap_or((text, "00:00")); // unwrap_or: a bare date is its midnight
    let mut date = date.split('-').map(str::parse::<i64>);
    let (year, month, day) = (date.next()?.ok()?, date.next()?.ok()?, date.next()?.ok()?);
    let mut time = time.split(':');
    let hour: i64 = time.next()?.parse().ok()?;
    let minute: i64 = time.next()?.parse().ok()?;
    let seconds: f64 = time.next().map_or(Some(0.0), |s| s.parse().ok())?; // map_or: minutes alone are :00
    let ms = (days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60) * 1_000 + (seconds * 1_000.0) as i64;
    u64::try_from(ms).ok()
}

/// Unix milliseconds as `2026-10-26T22:00:00Z`.
pub fn iso(ms: u64) -> String {
    let seconds = (ms / 1_000) as i64;
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    let of_day = seconds.rem_euclid(86_400);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", of_day / 3_600, of_day % 3_600 / 60, of_day % 60)
}

/// Unix milliseconds as `22:00:00`, the table's time column.
pub fn hms(ms: u64) -> String {
    iso(ms)[11..19].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a `--since` that lands on the wrong instant (an off-by-one day or a
    // dropped minute) silently selects a different night of acts.
    #[test]
    fn utc_text_round_trips_through_unix_milliseconds() {
        let at = parse("2026-10-26T22:00Z");
        assert_eq!(at, Some(1_793_052_000_000));
        assert_eq!(parse("2026-10-26T22:00:00.250Z"), Some(1_793_052_000_250));
        assert_eq!(iso(1_793_052_000_250), "2026-10-26T22:00:00Z");
        assert_eq!(hms(1_793_067_282_757), "02:14:42");
        assert_eq!(parse("not a time"), None);
    }
}
