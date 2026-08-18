use chrono::{DateTime, Duration, NaiveTime, Utc};
use chrono_tz::Europe::Vienna;

pub enum Frequency {
    Never,
    Immediately,
    Days(i32),
}

pub fn parse_frequency(value: &str) -> Option<Frequency> {
    match value {
        "never" => Some(Frequency::Never),
        "immediately" => Some(Frequency::Immediately),
        n => n
            .parse::<i32>()
            .ok()
            .filter(|&d| (1..=7).contains(&d))
            .map(Frequency::Days),
    }
}

pub fn next_digest_after(
    anchor: DateTime<Utc>,
    digest_hour: i32,
    frequency_days: i32,
    t: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let hour = u8::try_from(digest_hour.clamp(0, 23)).ok()?;
    // first digest moment on/after the anchor day at digest_hour (Vienna time)
    let mut candidate = anchor
        .with_timezone(&Vienna)
        .date_naive()
        .and_time(NaiveTime::from_hms_opt(hour.into(), 0, 0)?)
        .and_local_timezone(Vienna)
        .single()?
        .with_timezone(&Utc);
    if candidate < anchor {
        candidate += Duration::days(1);
    }
    // step forward by frequency_days until strictly after `t`
    let step = Duration::days(frequency_days.max(1) as i64);
    while candidate <= t {
        candidate += step;
    }
    Some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(day: u32, hour: u32) -> DateTime<Utc> {
        Vienna
            .with_ymd_and_hms(2026, 8, day, hour, 0, 0)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn daily_digest_first_after_t() {
        // anchor 18.08 09:00, frequency 1 day. First digest strictly after
        // 18.08 10:00 is 19.08 09:00.
        let anchor = at(18, 9);
        let t = at(18, 10);
        let next = next_digest_after(anchor, 9, 1, t).unwrap();
        assert_eq!(next, at(19, 9));
    }

    #[test]
    fn three_day_digest_steps_correctly() {
        // anchor 16.08 09:00, frequency 3 days => digest moments 16/19/22.08 09:00.
        // First digest strictly after 19.08 08:00 is 19.08 09:00.
        let anchor = at(16, 9);
        let t = at(19, 8);
        let next = next_digest_after(anchor, 9, 3, t).unwrap();
        assert_eq!(next, at(19, 9));
    }

    #[test]
    fn due_is_defined_by_next_digest_on_or_before_now() {
        // Circular helper test pinning the contract later tasks rely on:
        // a batch created at `created_at` is due at `now` iff
        // next_digest_after(anchor, hour, days, created_at) <= now.
        let anchor = at(16, 9);
        let created_at = at(19, 8);
        let digest_after_create = next_digest_after(anchor, 9, 3, created_at).unwrap();
        assert_eq!(digest_after_create, at(19, 9));
        assert!(digest_after_create <= at(19, 9)); // due at 19.08 09:00
        assert!(digest_after_create > at(19, 8)); // not due at 19.08 08:00
    }
}
