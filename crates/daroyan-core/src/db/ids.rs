use serde::{Deserialize, Serialize};

macro_rules! id_type {
    ($name:ident) => {
        #[derive(
            Clone,
            Debug,
            PartialEq,
            Eq,
            Hash,
            Serialize,
            Deserialize,
            utoipa::ToSchema
        )]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<&str> for $name {
            fn from(id: &str) -> Self {
                Self(id.to_owned())
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

id_type!(ServerId);
id_type!(InstallTokenId);
id_type!(CommandId);
id_type!(IntegrationId);
id_type!(AlertMuteId);
id_type!(AlertRuleId);
id_type!(AlertId);
id_type!(UserId);
id_type!(UptimeCheckId);
id_type!(CronId);
id_type!(StatusPageId);
id_type!(IncidentId);
id_type!(IncidentUpdateId);
id_type!(IngestTokenId);
id_type!(McpTokenId);
id_type!(MemberId);
