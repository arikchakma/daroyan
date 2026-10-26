use chrono::{DateTime, Utc};
use serde::Serializer;

pub const ISO_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.3fZ";

pub fn serialize<S: Serializer>(
    value: &DateTime<Utc>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_str(&value.format(ISO_FORMAT))
}

pub mod option {
    use chrono::{DateTime, Utc};
    use serde::Serializer;

    pub fn serialize<S: Serializer>(
        value: &Option<DateTime<Utc>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(value) => super::serialize(value, serializer),
            None => serializer.serialize_none(),
        }
    }
}
