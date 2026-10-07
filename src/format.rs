//! Figures and dates in words, the way the dashboard shows them.
//!
//! Every date goes through a [Zone] rather than straight to the system's clock, so the same instant reads
//! as the listener's own evening on screen and as a fixed, known time in the tests.

use chrono::{DateTime, Datelike, FixedOffset, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};

/// The clock the listener reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    /// The system's own, with its daylight saving: an instant from March is read with March's offset.
    Local,
    /// A fixed offset, for the tests and for pictures of the window that should not depend on the machine.
    Fixed(FixedOffset),
}

impl Zone {
    pub fn east_minutes(minutes: i32) -> Zone {
        Zone::Fixed(FixedOffset::east_opt(minutes * 60).expect("an offset within a day"))
    }

    pub fn local_time(self, at: DateTime<Utc>) -> NaiveDateTime {
        match self {
            Zone::Local => at.with_timezone(&Local).naive_local(),
            Zone::Fixed(offset) => at.with_timezone(&offset).naive_local(),
        }
    }

    pub fn today(self, now: DateTime<Utc>) -> NaiveDate {
        self.local_time(now).date()
    }

    /// Minutes east of UTC at [at], which is what the service's `tz` parameter wants.
    pub fn offset_minutes(self, at: DateTime<Utc>) -> i32 {
        let seconds = match self {
            Zone::Local => Local
                .offset_from_utc_datetime(&at.naive_utc())
                .local_minus_utc(),
            Zone::Fixed(offset) => offset.local_minus_utc(),
        };
        seconds / 60
    }
}

/// Whole numbers with separators, so six figures stay readable at a glance: 1,553.
pub fn count(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Hours listened, for the big figure: a decimal while there are few enough that the tenth matters, whole
/// hours after that. Nothing at all is "0", not "0.0".
pub fn hours(ms_played: u64) -> String {
    let hours = ms_played as f64 / 3_600_000.0;
    if ms_played == 0 {
        "0".into()
    } else if hours < 10.0 {
        let tenths = (hours * 10.0).round() / 10.0;
        if tenths == 0.0 {
            // Some listening, less than six minutes of it: rounding to nothing would say none happened.
            "0.1".into()
        } else {
            format!("{tenths:.1}")
        }
    } else {
        count(hours.round() as u64)
    }
}

/// A length of listening for a row: "21 min", "3 h", "20 h 27 min".
pub fn minutes(total: u64) -> String {
    let (h, m) = (total / 60, total % 60);
    match (h, m) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{} h", count(h)),
        (h, m) => format!("{} h {m} min", count(h)),
    }
}

fn plural(n: u64, one: &str, many: &str) -> String {
    format!("{} {}", count(n), if n == 1 { one } else { many })
}

pub fn plays(n: u64) -> String {
    plural(n, "play", "plays")
}

pub fn songs(n: u64) -> String {
    plural(n, "song", "songs")
}

pub fn days(n: u64) -> String {
    plural(n, "day", "days")
}

/// A share of the plays: "41%", and "<1%" for one that rounds to nothing but is not nothing.
pub fn percent(share: f64) -> String {
    let rounded = (share * 100.0).round();
    if share > 0.0 && rounded < 1.0 {
        "<1%".into()
    } else {
        format!("{rounded:.0}%")
    }
}

/// When a song was played, as near as is useful: "Today, 09:00", "Yesterday, 22:25", "Monday, 21:12" within
/// the week, "5 Oct, 22:12" within the year, and "14 Aug 2025" before that.
pub fn played_at(at: DateTime<Utc>, now: DateTime<Utc>, zone: Zone) -> String {
    let local = zone.local_time(at);
    let today = zone.today(now);
    let days_ago = (today - local.date()).num_days();
    let time = local.format("%H:%M");
    match days_ago {
        0 => format!("Today, {time}"),
        1 => format!("Yesterday, {time}"),
        2..=6 => format!("{}, {time}", local.format("%A")),
        _ if local.year() == today.year() => format!("{}, {time}", local.format("%-d %b")),
        _ => local.format("%-d %b %Y").to_string(),
    }
}

/// When the figures were fetched: "Updated at 21:33", or "Updated yesterday, 21:33".
pub fn updated(at: DateTime<Utc>, now: DateTime<Utc>, zone: Zone) -> String {
    if zone.today(now) == zone.local_time(at).date() {
        return format!("Updated at {}", zone.local_time(at).format("%H:%M"));
    }
    let when = played_at(at, now, zone);
    let mut chars = when.chars();
    let lowered: String = match chars.next() {
        // "Yesterday" and "Today" are only capitalised because they start a phrase; a weekday or a month is
        // a name and keeps its capital.
        Some(first) if when.starts_with("Yesterday") => first.to_lowercase().chain(chars).collect(),
        _ => when.clone(),
    };
    format!("Updated {lowered}")
}

/// The days a range covers, under the title: "1–7 Oct 2026", "8 Sep – 7 Oct 2026", or "Since 14 Aug 2025"
/// for all of it.
pub fn span(
    from: Option<DateTime<Utc>>,
    first_play: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    zone: Zone,
) -> String {
    let today = zone.today(now);
    match from {
        Some(from) => {
            let start = zone.local_time(from).date();
            if start.year() != today.year() {
                format!(
                    "{} – {}",
                    start.format("%-d %b %Y"),
                    today.format("%-d %b %Y")
                )
            } else if start.month() != today.month() {
                format!("{} – {}", start.format("%-d %b"), today.format("%-d %b %Y"))
            } else if start != today {
                format!("{}–{}", start.format("%-d"), today.format("%-d %b %Y"))
            } else {
                today.format("%-d %b %Y").to_string()
            }
        }
        None => match first_play {
            Some(first) => format!(
                "Since {}",
                zone.local_time(first).date().format("%-d %b %Y")
            ),
            None => "Everything so far".into(),
        },
    }
}

/// A day of the timeline, for the label under the bars: "6 Oct".
pub fn short_day(date: NaiveDate) -> String {
    date.format("%-d %b").to_string()
}

/// A day of the timeline, for its tooltip: "Tuesday 6 October".
pub fn long_day(date: NaiveDate) -> String {
    date.format("%A %-d %B").to_string()
}

/// A month of the timeline, under the bars: "Oct", with the year on January and on the first bar, so the
/// axis always says which year it is in.
pub fn short_month(date: NaiveDate, first: bool) -> String {
    if first || date.month() == 1 {
        date.format("%b %Y").to_string()
    } else {
        date.format("%b").to_string()
    }
}

/// A month of the timeline, for its tooltip: "October 2026".
pub fn long_month(date: NaiveDate) -> String {
    date.format("%B %Y").to_string()
}

/// An hour of the day as the span it covers: "20:00–21:00".
pub fn hour_span(hour: u32) -> String {
    format!("{:02}:00–{:02}:00", hour, (hour + 1) % 24)
}

/// 1 for Monday to 7 for Sunday.
pub fn weekday(day: u32) -> &'static str {
    [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ][(day.clamp(1, 7) - 1) as usize]
}

pub fn weekday_short(day: u32) -> &'static str {
    &weekday(day)[..3]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .unwrap()
            .with_timezone(&Utc)
    }

    /// London in summer.
    fn bst() -> Zone {
        Zone::east_minutes(60)
    }

    #[test]
    fn counts_have_separators() {
        assert_eq!(count(0), "0");
        assert_eq!(count(999), "999");
        assert_eq!(count(1_553), "1,553");
        assert_eq!(count(12_345_678), "12,345,678");
    }

    #[test]
    fn hours_keep_the_tenth_only_while_it_matters() {
        assert_eq!(hours(0), "0");
        assert_eq!(hours(60_000), "0.1"); // a minute is not nothing
        assert_eq!(hours(5_835_196), "1.6");
        assert_eq!(hours(9 * 3_600_000 + 2_000_000), "9.6");
        assert_eq!(hours(280_800_000), "78");
        assert_eq!(hours(4_334_400_000), "1,204");
    }

    #[test]
    fn minutes_read_as_hours_once_there_are_enough() {
        assert_eq!(minutes(0), "0 min");
        assert_eq!(minutes(21), "21 min");
        assert_eq!(minutes(180), "3 h");
        assert_eq!(minutes(1_227), "20 h 27 min");
    }

    #[test]
    fn one_is_singular() {
        assert_eq!(plays(1), "1 play");
        assert_eq!(plays(1_553), "1,553 plays");
        assert_eq!(songs(1), "1 song");
        assert_eq!(days(12), "12 days");
    }

    #[test]
    fn shares_never_round_to_nothing() {
        assert_eq!(percent(0.0), "0%");
        assert_eq!(percent(0.004), "<1%");
        assert_eq!(percent(0.409), "41%");
        assert_eq!(percent(1.0), "100%");
    }

    #[test]
    fn plays_are_dated_on_the_listeners_own_clock() {
        let now = utc("2026-10-07T12:00:00Z");
        assert_eq!(
            played_at(utc("2026-10-07T09:00:00.405Z"), now, bst()),
            "Today, 10:00"
        );
        // Twenty past eleven at night in UTC is twenty past midnight in London: the next day, not this one.
        assert_eq!(
            played_at(utc("2026-10-06T23:20:00Z"), now, bst()),
            "Today, 00:20"
        );
        assert_eq!(
            played_at(utc("2026-10-06T22:25:02Z"), now, bst()),
            "Yesterday, 23:25"
        );
        assert_eq!(
            played_at(utc("2026-10-03T19:00:00Z"), now, bst()),
            "Saturday, 20:00"
        );
        assert_eq!(
            played_at(utc("2026-08-14T09:03:56Z"), now, bst()),
            "14 Aug, 10:03"
        );
        assert_eq!(
            played_at(utc("2025-08-14T09:03:56Z"), now, bst()),
            "14 Aug 2025"
        );
        // And the other side of the world, where it is already the eighth.
        let auckland = Zone::east_minutes(13 * 60);
        assert_eq!(
            played_at(utc("2026-10-07T12:30:00Z"), now, auckland),
            "Today, 01:30"
        );
        assert_eq!(
            played_at(utc("2026-10-07T09:00:00Z"), now, auckland),
            "Yesterday, 22:00"
        );
    }

    #[test]
    fn the_time_fetched_says_the_day_when_it_was_not_today() {
        let now = utc("2026-10-07T20:00:00Z");
        assert_eq!(
            updated(utc("2026-10-07T18:33:00Z"), now, bst()),
            "Updated at 19:33"
        );
        assert_eq!(
            updated(utc("2026-10-06T18:33:00Z"), now, bst()),
            "Updated yesterday, 19:33"
        );
        assert_eq!(
            updated(utc("2026-10-04T18:33:00Z"), now, bst()),
            "Updated Sunday, 19:33"
        );
    }

    #[test]
    fn spans_say_only_as_much_of_the_date_as_changes() {
        let now = utc("2026-10-07T12:00:00Z");
        let zone = bst();
        // As the service sends them: local midnight, as UTC.
        assert_eq!(
            span(Some(utc("2026-09-30T23:00:00Z")), None, now, zone),
            "1–7 Oct 2026"
        );
        assert_eq!(
            span(Some(utc("2026-09-07T23:00:00Z")), None, now, zone),
            "8 Sep – 7 Oct 2026"
        );
        assert_eq!(
            span(Some(utc("2025-10-07T23:00:00Z")), None, now, zone),
            "8 Oct 2025 – 7 Oct 2026"
        );
        assert_eq!(
            span(None, Some(utc("2025-08-14T09:03:56Z")), now, zone),
            "Since 14 Aug 2025"
        );
        assert_eq!(span(None, None, now, zone), "Everything so far");
    }

    #[test]
    fn the_offset_sent_is_minutes_east() {
        let now = utc("2026-10-07T12:00:00Z");
        assert_eq!(bst().offset_minutes(now), 60);
        assert_eq!(Zone::east_minutes(-300).offset_minutes(now), -300);
        assert_eq!(Zone::east_minutes(345).offset_minutes(now), 345);
        // Whatever this machine's zone is, it is a real one.
        assert!(Zone::Local.offset_minutes(now).abs() <= 14 * 60);
    }

    #[test]
    fn the_axis_and_its_tooltips() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 6).unwrap();
        assert_eq!(short_day(day), "6 Oct");
        assert_eq!(long_day(day), "Tuesday 6 October");
        assert_eq!(short_month(day, false), "Oct");
        assert_eq!(short_month(day, true), "Oct 2026");
        assert_eq!(
            short_month(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), false),
            "Jan 2026"
        );
        assert_eq!(long_month(day), "October 2026");
        assert_eq!(hour_span(20), "20:00–21:00");
        assert_eq!(hour_span(23), "23:00–00:00");
        assert_eq!(weekday(1), "Monday");
        assert_eq!(weekday_short(7), "Sun");
    }
}
