pub mod bankid;
pub mod open_banking;

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationStatus {
    pub available: bool,
    pub provider: String,
    pub manual_fallback_hint: String,
}

pub fn open_banking_status() -> IntegrationStatus {
    open_banking::status()
}

pub fn bankid_status() -> IntegrationStatus {
    bankid::status()
}

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationStatusResponse {
    pub open_banking: IntegrationStatus,
    pub bankid: IntegrationStatus,
}

pub fn status_response() -> IntegrationStatusResponse {
    IntegrationStatusResponse {
        open_banking: open_banking_status(),
        bankid: bankid_status(),
    }
}
