use serde::{Deserialize, Serialize};
use strum::{EnumString, IntoStaticStr};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, IntoStaticStr, EnumString,
)]
pub enum Status {
    Rejected,
    Passed,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PreSubmitStatus {
    Rejected {
        reason: PreSubmitRejectReason,
    },
    Passed {
        lead: bool,
        choir: bool,
        harmony: bool,
    },
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    IntoStaticStr,
    EnumString,
    Serialize,
    Deserialize,
)]
pub enum PreSubmitRejectReason {
    DeviceOrEnvironment,
    RequirementNotMet,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SubmitStatus {
    Rejected {
        reason: PreSubmitRejectReason,
        detail: Option<Box<str>>,
    },
    Passed,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    IntoStaticStr,
    EnumString,
    Serialize,
    Deserialize,
)]
pub enum SubmitRejectReason {
    DeviceOrEnvironment,
    RequirementNotMet,
    Other,
}
