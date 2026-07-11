use super::IntegrationStatus;

pub fn status() -> IntegrationStatus {
    #[cfg(feature = "bankid")]
    {
        IntegrationStatus {
            available: false,
            provider: "bankid-stub".to_string(),
            manual_fallback_hint:
                "BankID is not configured. Continue with local export and manual review."
                    .to_string(),
        }
    }
    #[cfg(not(feature = "bankid"))]
    {
        IntegrationStatus {
            available: false,
            provider: "disabled".to_string(),
            manual_fallback_hint:
                "BankID is disabled. Use offline export packages for identity-sensitive workflows."
                    .to_string(),
        }
    }
}
