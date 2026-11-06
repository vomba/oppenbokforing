//! Immutable compiled identities. Historical lookup never depends on activation.
use sha2::{Digest, Sha256};

pub(crate) struct ValidationCase {
    pub id: &'static str,
    pub rule_id: &'static str,
    pub manifest: &'static str,
}
impl ValidationCase {
    pub fn digest(&self) -> String {
        format!("{:x}", Sha256::digest(self.manifest.as_bytes()))
    }
}
pub(crate) const INVOICE_V1: ValidationCase = ValidationCase {
    id: "invoice-recognition-se-service25-2026-exact-v1",
    rule_id: "rv-2026-domestic-service25-exact-v1",
    manifest: include_str!(
        "../../../fixtures/accounting-validation/2026-domestic-service25-exact.json"
    ),
};
pub(crate) const INVOICE_V2: ValidationCase = ValidationCase {
    id: "invoice-recognition-se-service25-2026-exact-v2",
    rule_id: "rv-2026-method-recognition-v2",
    manifest: include_str!(
        "../../../fixtures/accounting-validation/2026-domestic-service25-exact-v2.json"
    ),
};
pub(crate) const CASH_V1: ValidationCase = ValidationCase {
    id: "cash-recognition-se-service25-2026-pdf-full-v1",
    rule_id: "rv-2026-method-recognition-v2",
    manifest: include_str!("../../../fixtures/accounting-validation/2026-cash-pdf-full.json"),
};
pub(crate) fn get(id: &str) -> Option<&'static ValidationCase> {
    match id {
        id if id == INVOICE_V1.id => Some(&INVOICE_V1),
        id if id == INVOICE_V2.id => Some(&INVOICE_V2),
        id if id == CASH_V1.id => Some(&CASH_V1),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn three_immutable_cases_keep_exact_reviewed_raw_manifest_hashes() {
        for (case, digest) in [
            (
                &INVOICE_V1,
                "cb00f554e7ee3697b8461309e7184fd88b3a0aa70300f3f27ead9f6ca98b8594",
            ),
            (
                &INVOICE_V2,
                "5dbd4e6bd327f01112179ce325f086d9717f738d976065fbf4ec2268edd58cec",
            ),
            (
                &CASH_V1,
                "08d21e5dc4a055698b6a180e665141770bdb7f2c7325eea4e6d5bee4d54e7f7b",
            ),
        ] {
            assert_eq!(case.digest(), digest);
            assert_eq!(get(case.id).unwrap().rule_id, case.rule_id);
            let manifest: serde_json::Value = serde_json::from_str(case.manifest).unwrap();
            assert_eq!(manifest["validationId"], case.id);
            assert_eq!(manifest["ruleVersionId"], case.rule_id);
        }
        assert!(get("unknown").is_none());
    }
}
