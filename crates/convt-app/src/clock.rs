//! Local wall-clock times for the Activity list, without a date library.

/// A moment in local time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Local {
    pub year: i32,
    /// 1 to 12.
    pub month: u32,
    /// 1 to 31.
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
}

impl Local {
    /// The local time of `unix` seconds. Falls back to UTC where the system
    /// can't say.
    pub fn at(unix: i64) -> Self {
        local(unix).unwrap_or_else(|| utc(unix))
    }

    pub fn now() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        Self::at(now)
    }

    fn date(&self) -> (i32, u32, u32) {
        (self.year, self.month, self.day)
    }

    /// "4:08 PM".
    pub fn time(&self) -> String {
        let (hour, half) = match self.hour {
            0 => (12, "AM"),
            h @ 1..12 => (h, "AM"),
            12 => (12, "PM"),
            h => (h - 12, "PM"),
        };
        format!("{hour}:{:02} {half}", self.minute)
    }

    /// "Today", "Yesterday" or "Sep 28", relative to `today`.
    pub fn day_label(&self, today: &Local) -> String {
        if self.date() == today.date() {
            return "Today".into();
        }
        if self.date() == utc(days_from_civil(today.date()) * 86_400 - 86_400).date() {
            return "Yesterday".into();
        }
        const MONTHS: [&str; 12] = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let month = MONTHS[(self.month as usize).clamp(1, 12) - 1];
        if self.year == today.year {
            format!("{month} {}", self.day)
        } else {
            format!("{month} {}, {}", self.day, self.year)
        }
    }
}

#[cfg(unix)]
fn local(unix: i64) -> Option<Local> {
    // time_t is 32 bits on some systems.
    #[allow(clippy::useless_conversion)]
    let t: libc::time_t = unix.try_into().ok()?;
    // SAFETY: localtime_r only writes the `tm` we pass it.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let ok = unsafe { !libc::localtime_r(&t, &mut tm).is_null() };
    ok.then(|| Local {
        year: tm.tm_year + 1900,
        month: (tm.tm_mon + 1) as u32,
        day: tm.tm_mday as u32,
        hour: tm.tm_hour as u32,
        minute: tm.tm_min as u32,
    })
}

#[cfg(not(unix))]
fn local(_unix: i64) -> Option<Local> {
    None
}

fn utc(unix: i64) -> Local {
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
    Local {
        year,
        month,
        day,
        hour: (secs / 3600) as u32,
        minute: (secs % 3600 / 60) as u32,
    }
}

fn days_from_civil((year, month, day): (i32, u32, u32)) -> i64 {
    let y = i64::from(year) - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        let at = |y, mo, d, h, mi| Local {
            year: y,
            month: mo,
            day: d,
            hour: h,
            minute: mi,
        };
        assert_eq!(at(2026, 10, 2, 16, 8).time(), "4:08 PM");
        assert_eq!(at(2026, 10, 2, 0, 5).time(), "12:05 AM");
        assert_eq!(at(2026, 10, 2, 12, 0).time(), "12:00 PM");
        let today = at(2026, 10, 2, 9, 0);
        assert_eq!(at(2026, 10, 2, 1, 0).day_label(&today), "Today");
        assert_eq!(at(2026, 10, 1, 23, 0).day_label(&today), "Yesterday");
        assert_eq!(at(2026, 9, 28, 1, 0).day_label(&today), "Sep 28");
        assert_eq!(at(2025, 9, 28, 1, 0).day_label(&today), "Sep 28, 2025");
        let new_year = at(2026, 1, 1, 9, 0);
        assert_eq!(at(2025, 12, 31, 9, 0).day_label(&new_year), "Yesterday");
        assert_eq!(utc(0), at(1970, 1, 1, 0, 0));
        assert_eq!(utc(1_790_966_880), at(2026, 10, 2, 18, 48));
    }
}
