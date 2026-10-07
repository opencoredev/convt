//! Calendar dates as `YYYY-MM-DD` strings and day numbers since 1970-01-01,
//! using Howard Hinnant's civil date algorithms. Licenses and the trial only
//! need whole days in UTC.

/// The day number of `YYYY-MM-DD`, or `None` if it isn't a valid date.
pub fn to_days(date: &str) -> Option<i64> {
    let mut parts = date.splitn(3, '-');
    let (y, m, d) = (parts.next()?, parts.next()?, parts.next()?);
    if y.len() != 4 || m.len() != 2 || d.len() != 2 {
        return None;
    }
    let (y, m, d): (i64, i64, i64) = (y.parse().ok()?, m.parse().ok()?, d.parse().ok()?);
    if !(1..=12).contains(&m) || d < 1 || d > days_in_month(y, m) {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

/// The `YYYY-MM-DD` date of a day number.
pub fn from_days(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Seconds since 1970-01-01 of an RFC 3339 time such as
/// `2026-10-07T12:30:05.123Z` or `2026-10-07T14:30:05+02:00`, as convt.app
/// sends its clock. Fractions of a second are dropped.
pub fn parse_rfc3339(text: &str) -> Option<i64> {
    let text = text.trim();
    let (day, rest) = text.split_at_checked(10)?;
    let days = to_days(day)?;
    let rest = rest.strip_prefix(['T', 't', ' '])?;
    let (clock, rest) = rest.split_at_checked(8)?;
    let mut parts = clock.split(':');
    let mut field = |max: i64| -> Option<i64> {
        let part = parts.next()?;
        let n: i64 = (part.len() == 2 && part.bytes().all(|b| b.is_ascii_digit()))
            .then(|| part.parse().ok())??;
        (n <= max).then_some(n)
    };
    let (h, m, s) = (field(23)?, field(59)?, field(60)?);
    let rest = match rest.strip_prefix('.') {
        Some(fraction) => fraction.trim_start_matches(|c: char| c.is_ascii_digit()),
        None => rest,
    };
    let offset = match rest {
        "Z" | "z" => 0,
        _ => {
            let (sign, hm) = match rest.split_at_checked(1)? {
                ("+", hm) => (1, hm),
                ("-", hm) => (-1, hm),
                _ => return None,
            };
            let (oh, om) = hm.split_once(':')?;
            let (oh, om): (i64, i64) = (oh.parse().ok()?, om.parse().ok()?);
            if oh.to_string().len() > 2 || oh > 23 || om > 59 {
                return None;
            }
            sign * (oh * 3600 + om * 60)
        }
    };
    Some(days * 86_400 + h * 3600 + m * 60 + s - offset)
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        4 | 6 | 9 | 11 => 30,
        2 if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) => 29,
        2 => 28,
        _ => 31,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_times() {
        let base = to_days("2026-10-07").unwrap() * 86_400;
        assert_eq!(parse_rfc3339("2026-10-07T00:00:00Z"), Some(base));
        assert_eq!(
            parse_rfc3339("2026-10-07T12:30:05.123Z"),
            Some(base + 12 * 3600 + 30 * 60 + 5)
        );
        assert_eq!(
            parse_rfc3339("2026-10-07T14:30:05+02:00"),
            Some(base + 12 * 3600 + 30 * 60 + 5)
        );
        assert_eq!(parse_rfc3339("2026-10-06T23:00:00-01:00"), Some(base));
        for bad in [
            "",
            "2026-10-07",
            "2026-10-07T25:00:00Z",
            "2026-10-07T12:00:00",
            "2026-13-07T12:00:00Z",
            "2026-10-07T1:00:00Z",
            "2026-10-07T12:00:00+2",
        ] {
            assert_eq!(parse_rfc3339(bad), None, "{bad}");
        }
    }
}
