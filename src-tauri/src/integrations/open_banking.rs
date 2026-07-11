use super::IntegrationStatus;

pub fn status() -> IntegrationStatus {
    #[cfg(feature = "open-banking")]
    {
        IntegrationStatus {
            available: false,
            provider: "open-banking-stub".to_string(),
            manual_fallback_hint:
                "Open Banking is not configured. Use CSV import and manual reconciliation."
                    .to_string(),
        }
    }
    #[cfg(not(feature = "open-banking"))]
    {
        IntegrationStatus {
            available: false,
            provider: "disabled".to_string(),
            manual_fallback_hint:
                "Open Banking is disabled. Use CSV import, SIE export, or accountant package."
                    .to_string(),
        }
    }
}
