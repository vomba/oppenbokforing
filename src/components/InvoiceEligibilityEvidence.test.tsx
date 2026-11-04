import { render, screen, within } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { InvoiceEligibilityEvidence } from "./InvoiceEligibilityEvidence"
import evidence from "../../fixtures/accounting-validation/2026-domestic-service25-exact.json"
import type { InvoiceEligibilityAssessment } from "../lib/bindings"

const assessment: InvoiceEligibilityAssessment = {
  validationId: evidence.validationId,
  ruleVersionId: evidence.ruleVersionId,
  evidenceDigest: "retained-evidence-digest",
  reviewFingerprint: "retained-review-fingerprint",
  accountingYear: 2026,
  accountingDate: "2026-11-04",
  completedOn: "2026-11-03",
  deadlineRegime: "annual_may_12",
  periodKey: "2026",
  dueOn: "2027-05-12",
  coverageSummary: "Recognition only; not full invoice or tax preparation validation.",
}

describe("InvoiceEligibilityEvidence sources", () => {
  it("makes the bundled case's authoritative sources inspectable alongside its identity", () => {
    render(<InvoiceEligibilityEvidence assessment={assessment} locale="sv" />)
    const sources = screen.getByRole("region", { name: "Kontrollens källor" })
    for (const source of evidence.sourceReferences) {
      expect(within(sources).getByRole("link", { name: source.pinpoint })).toHaveAttribute("href", source.url)
    }
    expect(screen.getByText(/retained-evidence-digest/)).toBeInTheDocument()
  })

  it("does not substitute this case's sources for an unknown retained validation", () => {
    render(<InvoiceEligibilityEvidence assessment={{ ...assessment, validationId: "another-case" }} locale="en" />)
    expect(screen.queryByRole("region", { name: "Validation sources" })).not.toBeInTheDocument()
    expect(screen.getByText(/another-case/)).toBeInTheDocument()
  })
})
