use chrono::Utc;
use rand::Rng;

const ID_ALPHABET: &[u8] = b"0123456789abcdefghjkmnpqrstvwxyz";
const TIME_LENGTH: usize = 9;
const RANDOM_LENGTH: usize = 11;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prefix {
    Server,
    InstallToken,
    Command,
    Integration,
    AlertRule,
    Alert,
    AlertDelivery,
    AlertMute,
    User,
    Session,
    UptimeCheck,
    UptimeOutage,
    Cron,
    CronFailure,
    StatusPage,
    Incident,
    IncidentUpdate,
    FollowUp,
    IngestToken,
    McpToken,
    Member,
}

impl Prefix {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Server => "srv",
            Self::InstallToken => "itk",
            Self::Command => "cmd",
            Self::Integration => "int",
            Self::AlertRule => "rul",
            Self::Alert => "alr",
            Self::AlertDelivery => "dlv",
            Self::AlertMute => "mut",
            Self::User => "usr",
            Self::Session => "ses",
            Self::UptimeCheck => "chk",
            Self::UptimeOutage => "out",
            Self::Cron => "crn",
            Self::CronFailure => "fal",
            Self::StatusPage => "stp",
            Self::Incident => "inc",
            Self::IncidentUpdate => "upd",
            Self::FollowUp => "fup",
            Self::IngestToken => "igt",
            Self::McpToken => "mcp",
            Self::Member => "mem",
        }
    }
}

pub fn new_id(prefix: Prefix) -> String {
    let mut millis = u64::try_from(Utc::now().timestamp_millis()).unwrap_or_default();
    let mut time = [0_u8; TIME_LENGTH];
    for slot in time.iter_mut().rev() {
        *slot = ID_ALPHABET[(millis % 32) as usize];
        millis /= 32;
    }
    let mut rng = rand::rng();
    let random: String = (0..RANDOM_LENGTH)
        .map(|_| char::from(ID_ALPHABET[rng.random_range(0..ID_ALPHABET.len())]))
        .collect();
    format!("{}_{}{random}", prefix.as_str(), String::from_utf8_lossy(&time))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_have_the_prefix_and_sort_by_time() {
        let first = new_id(Prefix::Server);
        std::thread::sleep(std::time::Duration::from_millis(2));
        let second = new_id(Prefix::Server);
        assert!(first.starts_with("srv_"));
        assert_eq!(first.len(), 4 + TIME_LENGTH + RANDOM_LENGTH);
        assert!(first[..13] < second[..13]);
    }
}
