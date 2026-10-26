use chrono::{DateTime, Duration, Utc};
pub use chrono_tz::Tz;
use croner::Cron;
use croner::parser::{CronParser, Seconds, Year};

use crate::db::entities::crons::Model as CronModel;

pub const KIND_CRON: &str = "cron";
pub const KIND_EVERY: &str = "every";

#[derive(Clone, Debug)]
pub enum Schedule {
    Every(Duration),
    Cron { cron: Box<Cron>, time_zone: Tz },
}

pub fn parse_expression(expression: &str) -> Option<Cron> {
    CronParser::builder()
        .seconds(Seconds::Disallowed)
        .year(Year::Disallowed)
        .build()
        .parse(expression.trim())
        .ok()
}

impl Schedule {
    pub fn read(cron: &CronModel) -> Option<Self> {
        match cron.schedule_kind.as_str() {
            KIND_EVERY => {
                Some(Self::Every(Duration::seconds(i64::from(cron.interval_secs?))))
            }
            _ => Some(Self::Cron {
                cron: Box::new(parse_expression(cron.expression.as_deref()?)?),
                time_zone: cron.time_zone.parse().ok()?,
            }),
        }
    }

    pub fn next_after(&self, at: DateTime<Utc>) -> Option<DateTime<Utc>> {
        match self {
            Self::Every(period) => Some(at + *period),
            Self::Cron { cron, time_zone } => cron
                .find_next_occurrence(&at.with_timezone(time_zone), false)
                .ok()
                .map(|next| next.with_timezone(&Utc)),
        }
    }

    pub fn find_slot(
        &self,
        began_at: DateTime<Utc>,
        last_ping_at: Option<DateTime<Utc>>,
    ) -> Option<DateTime<Utc>> {
        match self {
            Self::Every(period) => last_ping_at.map(|at| at + *period),
            Self::Cron { cron, time_zone } => cron
                .find_previous_occurrence(&began_at.with_timezone(time_zone), true)
                .ok()
                .map(|slot| slot.with_timezone(&Utc)),
        }
    }
}

pub fn find_due(cron: &CronModel, schedule: &Schedule) -> Option<DateTime<Utc>> {
    let last_ping_at = cron.last_ping_at?;
    let anchor = cron
        .missed_slot_at
        .map_or(last_ping_at, |slot| slot.max(last_ping_at));
    schedule.next_after(anchor)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn at(hour: u32, minute: u32, second: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 16, hour, minute, second).unwrap()
    }

    fn make_cron(expression: &str, time_zone: &str) -> Schedule {
        Schedule::Cron {
            cron: Box::new(parse_expression(expression).unwrap()),
            time_zone: time_zone.parse().unwrap(),
        }
    }

    #[test]
    fn reads_five_field_expressions_only() {
        assert!(parse_expression("*/5 * * * *").is_some());
        assert!(parse_expression("@hourly").is_some());
        assert!(parse_expression("0 2 * * MON-FRI").is_some());
        assert!(parse_expression("0 */5 * * * *").is_none());
        assert!(parse_expression("every day").is_none());
        assert!(parse_expression("").is_none());
    }

    #[test]
    fn finds_the_next_run_in_the_time_zone() {
        let every_five = make_cron("*/5 * * * *", "UTC");
        assert_eq!(every_five.next_after(at(14, 0, 2)), Some(at(14, 5, 0)));
        assert_eq!(every_five.next_after(at(14, 5, 0)), Some(at(14, 10, 0)));
        let dhaka = make_cron("0 2 * * *", "Asia/Dhaka");
        assert_eq!(dhaka.next_after(at(14, 0, 0)), Some(at(20, 0, 0)));
        let period = Schedule::Every(Duration::minutes(10));
        assert_eq!(period.next_after(at(14, 0, 2)), Some(at(14, 10, 2)));
    }

    #[test]
    fn places_a_run_in_its_slot() {
        let every_five = make_cron("*/5 * * * *", "UTC");
        assert_eq!(every_five.find_slot(at(14, 0, 41), None), Some(at(14, 0, 0)));
        assert_eq!(every_five.find_slot(at(14, 5, 0), None), Some(at(14, 5, 0)));
        let period = Schedule::Every(Duration::minutes(10));
        assert_eq!(
            period.find_slot(at(14, 11, 0), Some(at(14, 0, 0))),
            Some(at(14, 10, 0))
        );
        assert_eq!(period.find_slot(at(14, 11, 0), None), None);
    }
}
