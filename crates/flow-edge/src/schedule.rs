//! When the site is open, and what that means for the estimate.
//!
//! Little's Law assumes a queue in a settled regime. Outside service hours
//! there is no queue and no service rate, so `W = L / λ` is not a degraded
//! estimate — it is not an estimate at all. An appliance that announced a
//! two-minute wait at three in the morning would be reporting a number it
//! has no basis for. The schedule is therefore a correctness feature first;
//! that it also spares the CPU and the SD card is a welcome side effect.
//!
//! # Local time is the only sensible unit
//!
//! An operator thinks in wall-clock time — "we serve from 11:30" — while the
//! appliance holds microsecond Unix timestamps. Converting between the two
//! needs a time zone, and it must be a real one: a fixed offset would shift
//! the whole schedule by an hour twice a year, when daylight saving starts
//! and ends. The zone is stored with the schedule rather than taken from the
//! host, so the configuration says what it means and does not depend on how
//! an image happened to be prepared.
//!
//! # What a schedule can express
//!
//! Any number of intervals per weekday — a restaurant serving lunch and
//! dinner has two — and closed date ranges for holidays and school breaks.
//! A weekday with no interval is simply closed, which is how weekends are
//! expressed. An interval never crosses midnight: a service that did would
//! be two intervals on two days, and allowing it would make every
//! comparison in this module ambiguous for no one's benefit.

use std::fmt;

use jiff::civil::{Date, Time};
use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};
use serde::{Deserialize, Serialize};

use crate::error::ScheduleError;

/// How far ahead [`ServiceWindow::next_change`] will look, in days.
///
/// A schedule with no opening at all inside two weeks is closed for
/// practical purposes; saying "no upcoming change" is more honest than
/// scanning a year to find one.
const HORIZON_DAYS: i8 = 14;

/// A time of day, to the minute, in the site's own time zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LocalTime {
    minutes: u16,
}

impl LocalTime {
    /// Builds a time from hours and minutes.
    ///
    /// # Errors
    ///
    /// [`ScheduleError::Time`] outside `00:00..=23:59`.
    pub fn new(hour: u8, minute: u8) -> Result<Self, ScheduleError> {
        if hour > 23 || minute > 59 {
            return Err(ScheduleError::Time(format!("{hour:02}:{minute:02}")));
        }
        Ok(Self {
            minutes: u16::from(hour) * 60 + u16::from(minute),
        })
    }

    /// Minutes since midnight.
    #[must_use]
    pub fn minutes_since_midnight(self) -> u16 {
        self.minutes
    }

    fn hour(self) -> i8 {
        i8::try_from(self.minutes / 60).unwrap_or(0)
    }

    fn minute(self) -> i8 {
        i8::try_from(self.minutes % 60).unwrap_or(0)
    }

    fn from_civil(time: Time) -> Self {
        Self {
            minutes: u16::try_from(i32::from(time.hour()) * 60 + i32::from(time.minute()))
                .unwrap_or(0),
        }
    }
}

impl fmt::Display for LocalTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}", self.minutes / 60, self.minutes % 60)
    }
}

impl Serialize for LocalTime {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for LocalTime {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

impl std::str::FromStr for LocalTime {
    type Err = ScheduleError;

    fn from_str(text: &str) -> Result<Self, ScheduleError> {
        let (hour, minute) = text
            .split_once(':')
            .ok_or_else(|| ScheduleError::Time(text.to_owned()))?;
        let hour: u8 = hour
            .parse()
            .map_err(|_| ScheduleError::Time(text.to_owned()))?;
        let minute: u8 = minute
            .parse()
            .map_err(|_| ScheduleError::Time(text.to_owned()))?;
        Self::new(hour, minute)
    }
}

/// One continuous stretch of service, within a single day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interval {
    /// When service starts, inclusive.
    pub from: LocalTime,
    /// When service ends, exclusive.
    pub to: LocalTime,
}

impl Interval {
    fn contains(self, time: LocalTime) -> bool {
        time >= self.from && time < self.to
    }
}

/// Opening intervals for each weekday.
///
/// Named fields rather than a positional array: this file is meant to be
/// readable, and repairable, by someone looking at it on a card.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeeklyHours {
    /// Intervals served on Mondays.
    #[serde(default)]
    pub monday: Vec<Interval>,
    /// Intervals served on Tuesdays.
    #[serde(default)]
    pub tuesday: Vec<Interval>,
    /// Intervals served on Wednesdays.
    #[serde(default)]
    pub wednesday: Vec<Interval>,
    /// Intervals served on Thursdays.
    #[serde(default)]
    pub thursday: Vec<Interval>,
    /// Intervals served on Fridays.
    #[serde(default)]
    pub friday: Vec<Interval>,
    /// Intervals served on Saturdays.
    #[serde(default)]
    pub saturday: Vec<Interval>,
    /// Intervals served on Sundays.
    #[serde(default)]
    pub sunday: Vec<Interval>,
}

impl WeeklyHours {
    /// The intervals served on a given date's weekday.
    #[must_use]
    pub fn on(&self, date: Date) -> &[Interval] {
        match date.weekday() {
            jiff::civil::Weekday::Monday => &self.monday,
            jiff::civil::Weekday::Tuesday => &self.tuesday,
            jiff::civil::Weekday::Wednesday => &self.wednesday,
            jiff::civil::Weekday::Thursday => &self.thursday,
            jiff::civil::Weekday::Friday => &self.friday,
            jiff::civil::Weekday::Saturday => &self.saturday,
            jiff::civil::Weekday::Sunday => &self.sunday,
        }
    }

    fn all(&self) -> [(&'static str, &Vec<Interval>); 7] {
        [
            ("monday", &self.monday),
            ("tuesday", &self.tuesday),
            ("wednesday", &self.wednesday),
            ("thursday", &self.thursday),
            ("friday", &self.friday),
            ("saturday", &self.saturday),
            ("sunday", &self.sunday),
        ]
    }
}

/// A stretch of days the site is closed whatever the weekly schedule says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Closure {
    /// First closed day, inclusive, as `YYYY-MM-DD`.
    pub from: String,
    /// Last closed day, inclusive.
    pub to: String,
    /// What it is, for the operator's benefit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl Closure {
    fn range(&self) -> Result<(Date, Date), ScheduleError> {
        let from: Date = self
            .from
            .parse()
            .map_err(|_| ScheduleError::Date(self.from.clone()))?;
        let to: Date = self
            .to
            .parse()
            .map_err(|_| ScheduleError::Date(self.to.clone()))?;
        if to < from {
            return Err(ScheduleError::ClosureOrder {
                from: self.from.clone(),
                to: self.to.clone(),
            });
        }
        Ok((from, to))
    }
}

/// When a site serves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceWindow {
    /// IANA name of the site's time zone, such as `Europe/Paris`.
    pub timezone: String,
    /// The recurring weekly schedule.
    pub weekly: WeeklyHours,
    /// Exceptional closed periods: holidays, school breaks.
    #[serde(default)]
    pub closures: Vec<Closure>,
}

/// Whether the site is serving, and when that next changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ServiceState {
    /// Whether the site is serving right now.
    pub open: bool,
    /// When this changes, in µs since the Unix epoch, if within the
    /// horizon.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changes_at_us: Option<u64>,
}

impl ServiceWindow {
    /// Checks that the schedule is usable.
    ///
    /// # Errors
    ///
    /// The first [`ScheduleError`] found: an unknown time zone, a malformed
    /// date, an interval that ends before it starts, or two intervals of
    /// one day that overlap.
    pub fn validate(&self) -> Result<(), ScheduleError> {
        self.zone()?;
        for closure in &self.closures {
            closure.range()?;
        }
        for (day, intervals) in self.weekly.all() {
            let mut sorted: Vec<Interval> = intervals.clone();
            sorted.sort_by_key(|interval| interval.from);
            for pair in sorted.windows(2) {
                if pair[1].from < pair[0].to {
                    return Err(ScheduleError::Overlap {
                        day,
                        first: format!("{}-{}", pair[0].from, pair[0].to),
                        second: format!("{}-{}", pair[1].from, pair[1].to),
                    });
                }
            }
            for interval in intervals {
                if interval.to <= interval.from {
                    return Err(ScheduleError::IntervalOrder {
                        day,
                        from: interval.from.to_string(),
                        to: interval.to.to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    fn zone(&self) -> Result<TimeZone, ScheduleError> {
        TimeZone::get(&self.timezone).map_err(|_| ScheduleError::TimeZone(self.timezone.clone()))
    }

    fn is_closed_on(&self, date: Date) -> bool {
        self.closures.iter().any(|closure| {
            closure
                .range()
                .is_ok_and(|(from, to)| date >= from && date <= to)
        })
    }

    /// Whether the site is serving at `at_us`.
    ///
    /// # Errors
    ///
    /// [`ScheduleError`] if the schedule cannot be interpreted.
    pub fn is_open(&self, at_us: u64) -> Result<bool, ScheduleError> {
        let zoned = self.zoned(at_us)?;
        let date = zoned.date();
        if self.is_closed_on(date) {
            return Ok(false);
        }
        let now = LocalTime::from_civil(zoned.time());
        Ok(self
            .weekly
            .on(date)
            .iter()
            .any(|interval| interval.contains(now)))
    }

    /// The current state and the next transition, if one is near.
    ///
    /// # Errors
    ///
    /// [`ScheduleError`] if the schedule cannot be interpreted.
    pub fn state(&self, at_us: u64) -> Result<ServiceState, ScheduleError> {
        Ok(ServiceState {
            open: self.is_open(at_us)?,
            changes_at_us: self.next_change(at_us)?,
        })
    }

    /// When the site next opens or closes, within [`HORIZON_DAYS`].
    ///
    /// Boundaries are resolved through the time zone with jiff's default
    /// disambiguation, so the hour that does not exist on the spring
    /// transition, and the hour that happens twice in autumn, both resolve
    /// to a single defined instant rather than failing.
    ///
    /// # Errors
    ///
    /// [`ScheduleError`] if the schedule cannot be interpreted.
    pub fn next_change(&self, at_us: u64) -> Result<Option<u64>, ScheduleError> {
        let zone = self.zone()?;
        let now = self.zoned(at_us)?;
        let open_now = self.is_open(at_us)?;

        let mut date = now.date();
        for _ in 0..HORIZON_DAYS {
            if !self.is_closed_on(date) {
                let mut boundaries: Vec<LocalTime> = self
                    .weekly
                    .on(date)
                    .iter()
                    .flat_map(|interval| [interval.from, interval.to])
                    .collect();
                boundaries.sort_unstable();

                for boundary in boundaries {
                    let Ok(time) = Time::new(boundary.hour(), boundary.minute(), 0, 0) else {
                        continue;
                    };
                    let Ok(zoned) = date.to_datetime(time).to_zoned(zone.clone()) else {
                        continue;
                    };
                    let candidate = zoned.timestamp();
                    let candidate_us = to_us(candidate);
                    if candidate_us > at_us && self.is_open(candidate_us)? != open_now {
                        return Ok(Some(candidate_us));
                    }
                }
            }
            let Ok(next) = date.tomorrow() else {
                break;
            };
            date = next;
        }
        Ok(None)
    }

    fn zoned(&self, at_us: u64) -> Result<Zoned, ScheduleError> {
        let seconds = i64::try_from(at_us / 1_000_000).unwrap_or(0);
        let nanos = i32::try_from((at_us % 1_000_000) * 1_000).unwrap_or(0);
        let timestamp =
            Timestamp::new(seconds, nanos).map_err(|_| ScheduleError::Instant(at_us))?;
        Ok(timestamp.to_zoned(self.zone()?))
    }
}

fn to_us(timestamp: Timestamp) -> u64 {
    let micros = timestamp.as_microsecond();
    u64::try_from(micros).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an interval from `HH:MM` strings.
    fn interval(from: &str, to: &str) -> Interval {
        Interval {
            from: from.parse().unwrap(),
            to: to.parse().unwrap(),
        }
    }

    /// A restaurant serving lunch on weekdays, in Paris.
    fn lunch_only() -> ServiceWindow {
        let midday = vec![interval("11:30", "14:00")];
        ServiceWindow {
            timezone: "Europe/Paris".into(),
            weekly: WeeklyHours {
                monday: midday.clone(),
                tuesday: midday.clone(),
                wednesday: midday.clone(),
                thursday: midday.clone(),
                friday: midday,
                ..WeeklyHours::default()
            },
            closures: Vec::new(),
        }
    }

    /// Microseconds for a local Paris wall-clock moment.
    fn paris(text: &str) -> u64 {
        let zoned: Zoned = format!("{text}[Europe/Paris]").parse().unwrap();
        to_us(zoned.timestamp())
    }

    #[test]
    fn a_time_round_trips_through_its_written_form() {
        let time: LocalTime = "11:30".parse().unwrap();
        assert_eq!(time.to_string(), "11:30");
        assert_eq!(time.minutes_since_midnight(), 690);
        assert_eq!("00:00".parse::<LocalTime>().unwrap().to_string(), "00:00");
        assert_eq!("23:59".parse::<LocalTime>().unwrap().to_string(), "23:59");
    }

    #[test]
    fn an_impossible_time_is_refused() {
        assert!("24:00".parse::<LocalTime>().is_err());
        assert!("11:60".parse::<LocalTime>().is_err());
        assert!("half past eleven".parse::<LocalTime>().is_err());
        assert!("1130".parse::<LocalTime>().is_err());
    }

    #[test]
    fn the_site_serves_inside_its_hours() {
        let window = lunch_only();
        // Thursday 2026-07-30.
        assert!(!window.is_open(paris("2026-07-30T11:29:59")).unwrap());
        assert!(window.is_open(paris("2026-07-30T11:30:00")).unwrap());
        assert!(window.is_open(paris("2026-07-30T13:59:59")).unwrap());
        assert!(
            !window.is_open(paris("2026-07-30T14:00:00")).unwrap(),
            "the closing time is exclusive"
        );
    }

    #[test]
    fn a_weekday_with_no_interval_is_simply_closed() {
        let window = lunch_only();
        // Saturday and Sunday carry no interval at all.
        assert!(!window.is_open(paris("2026-08-01T12:00:00")).unwrap());
        assert!(!window.is_open(paris("2026-08-02T12:00:00")).unwrap());
    }

    #[test]
    fn two_services_a_day_are_both_honoured() {
        let mut window = lunch_only();
        window.weekly.monday = vec![interval("11:30", "14:00"), interval("18:30", "20:30")];

        assert!(window.is_open(paris("2026-08-03T12:00:00")).unwrap());
        assert!(!window.is_open(paris("2026-08-03T16:00:00")).unwrap());
        assert!(window.is_open(paris("2026-08-03T19:00:00")).unwrap());
    }

    #[test]
    fn a_closure_overrides_the_weekly_schedule() {
        let mut window = lunch_only();
        window.closures.push(Closure {
            from: "2026-08-03".into(),
            to: "2026-08-21".into(),
            reason: Some("summer break".into()),
        });

        assert!(
            !window.is_open(paris("2026-08-03T12:00:00")).unwrap(),
            "first day"
        );
        assert!(
            !window.is_open(paris("2026-08-21T12:00:00")).unwrap(),
            "last day"
        );
        assert!(
            window.is_open(paris("2026-08-24T12:00:00")).unwrap(),
            "after"
        );
        assert!(
            window.is_open(paris("2026-07-31T12:00:00")).unwrap(),
            "before"
        );
    }

    #[test]
    fn hours_are_read_in_the_site_zone_not_in_utc() {
        let window = lunch_only();
        // 11:45 UTC is 13:45 in Paris in summer — open — while 13:45 UTC is
        // 15:45 there, which is not.
        let utc = |text: &str| {
            let zoned: Zoned = format!("{text}[UTC]").parse().unwrap();
            to_us(zoned.timestamp())
        };
        assert!(window.is_open(utc("2026-07-30T11:45:00")).unwrap());
        assert!(!window.is_open(utc("2026-07-30T13:45:00")).unwrap());
    }

    #[test]
    fn daylight_saving_does_not_shift_the_service() {
        // The whole reason a real time zone is stored rather than a fixed
        // offset: Paris is UTC+1 in winter and UTC+2 in summer, so a fixed
        // offset would move service by an hour twice a year.
        let window = lunch_only();
        // Friday in winter and Thursday in summer, both at local noon.
        assert!(window.is_open(paris("2026-01-30T12:00:00")).unwrap());
        assert!(window.is_open(paris("2026-07-30T12:00:00")).unwrap());
        // And both closed at local 15:00.
        assert!(!window.is_open(paris("2026-01-30T15:00:00")).unwrap());
        assert!(!window.is_open(paris("2026-07-30T15:00:00")).unwrap());
    }

    #[test]
    fn the_next_change_is_the_coming_opening() {
        let window = lunch_only();
        let at = paris("2026-07-30T09:00:00");
        let change = window.next_change(at).unwrap().expect("opens today");
        assert_eq!(change, paris("2026-07-30T11:30:00"));
    }

    #[test]
    fn the_next_change_is_the_coming_closing_while_open() {
        let window = lunch_only();
        let at = paris("2026-07-30T12:00:00");
        let change = window.next_change(at).unwrap().expect("closes today");
        assert_eq!(change, paris("2026-07-30T14:00:00"));
    }

    #[test]
    fn the_next_change_skips_the_weekend() {
        let window = lunch_only();
        // Friday after service: the next opening is Monday.
        let at = paris("2026-07-31T15:00:00");
        let change = window.next_change(at).unwrap().expect("opens Monday");
        assert_eq!(change, paris("2026-08-03T11:30:00"));
    }

    #[test]
    fn a_schedule_that_never_opens_reports_no_change() {
        let window = ServiceWindow {
            timezone: "Europe/Paris".into(),
            weekly: WeeklyHours::default(),
            closures: Vec::new(),
        };
        assert!(!window.is_open(paris("2026-07-30T12:00:00")).unwrap());
        assert_eq!(
            window.next_change(paris("2026-07-30T12:00:00")).unwrap(),
            None
        );
    }

    #[test]
    fn the_state_pairs_the_answer_with_its_expiry() {
        let window = lunch_only();
        let state = window.state(paris("2026-07-30T12:00:00")).unwrap();
        assert!(state.open);
        assert_eq!(state.changes_at_us, Some(paris("2026-07-30T14:00:00")));
    }

    #[test]
    fn an_unknown_time_zone_is_refused() {
        let window = ServiceWindow {
            timezone: "Mars/Olympus_Mons".into(),
            weekly: WeeklyHours::default(),
            closures: Vec::new(),
        };
        assert!(matches!(window.validate(), Err(ScheduleError::TimeZone(_))));
    }

    #[test]
    fn an_interval_ending_before_it_starts_is_refused() {
        let mut window = lunch_only();
        window.weekly.monday = vec![interval("14:00", "11:30")];
        assert!(matches!(
            window.validate(),
            Err(ScheduleError::IntervalOrder { .. })
        ));
    }

    #[test]
    fn overlapping_intervals_of_one_day_are_refused() {
        // Two services that overlap are a mistake, not a schedule: the
        // operator meant one longer interval.
        let mut window = lunch_only();
        window.weekly.monday = vec![interval("11:30", "14:00"), interval("13:00", "15:00")];
        assert!(matches!(
            window.validate(),
            Err(ScheduleError::Overlap { .. })
        ));
    }

    #[test]
    fn utc_is_a_zone_like_any_other() {
        // The dashboard pre-fills the zone from the browser, and a machine
        // set to UTC reports exactly that. Refusing it here would turn an
        // ordinary configuration into a dead end on those machines.
        let mut window = lunch_only();
        window.timezone = "UTC".into();
        window.validate().unwrap();
    }

    #[test]
    fn a_complaint_names_the_day_it_is_about() {
        // Seven days are edited on one screen; "these two overlap" without a
        // day leaves the reader to find which row it means.
        let mut window = lunch_only();
        window.weekly.thursday = vec![interval("08:00", "12:00"), interval("11:00", "14:00")];
        let message = window.validate().unwrap_err().to_string();
        assert!(message.starts_with("thursday:"), "{message}");

        let mut window = lunch_only();
        window.weekly.saturday = vec![interval("18:00", "09:00")];
        let message = window.validate().unwrap_err().to_string();
        assert!(message.starts_with("saturday:"), "{message}");
    }

    #[test]
    fn adjacent_intervals_are_allowed() {
        let mut window = lunch_only();
        window.weekly.monday = vec![interval("11:30", "14:00"), interval("14:00", "15:00")];
        window.validate().unwrap();
        assert!(window.is_open(paris("2026-08-03T14:00:00")).unwrap());
    }

    #[test]
    fn a_backwards_closure_is_refused() {
        let mut window = lunch_only();
        window.closures.push(Closure {
            from: "2026-08-21".into(),
            to: "2026-08-03".into(),
            reason: None,
        });
        assert!(matches!(
            window.validate(),
            Err(ScheduleError::ClosureOrder { .. })
        ));
    }

    #[test]
    fn a_malformed_closure_date_is_refused() {
        let mut window = lunch_only();
        window.closures.push(Closure {
            from: "the summer".into(),
            to: "2026-08-21".into(),
            reason: None,
        });
        assert!(matches!(window.validate(), Err(ScheduleError::Date(_))));
    }

    #[test]
    fn a_valid_schedule_round_trips_through_json() {
        let window = lunch_only();
        let text = serde_json::to_string(&window).unwrap();
        assert!(text.contains("\"11:30\""), "times stay readable: {text}");
        assert_eq!(
            serde_json::from_str::<ServiceWindow>(&text).unwrap(),
            window
        );
    }
}
