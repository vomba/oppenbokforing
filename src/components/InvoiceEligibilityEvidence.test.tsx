import { fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { InvoiceEligibilityEvidence } from "./InvoiceEligibilityEvidence"
import evidence from "../../fixtures/accounting-validation/2026-domestic-service25-exact.json"
import type { InvoiceEligibilityAssessment } from "../lib/bindings"

const { invoiceValidationSourceOpen } = vi.hoisted(() => ({ invoiceValidationSourceOpen: vi.fn() }))
vi.mock("../lib/commands", () => ({
  invoiceValidationSourceOpen,
  appErrorMessage: (_error: unknown, fallback: string) => fallback,
}))

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
  beforeEach(() => {
    invoiceValidationSourceOpen.mockReset().mockResolvedValue(undefined)
  })

  it("makes the bundled case's authoritative sources inspectable alongside its identity", () => {
    render(<InvoiceEligibilityEvidence assessment={assessment} locale="sv" />)
    const sources = screen.getByRole("region", { name: "Kontrollens källor" })
    for (const source of evidence.sourceReferences) {
      expect(within(sources).getByRole("link", { name: source.pinpoint })).toHaveAttribute("href", source.url)
    }
    expect(screen.getByText(/retained-evidence-digest/)).toBeInTheDocument()
    expect(invoiceValidationSourceOpen).not.toHaveBeenCalled()
  })

  it("opens a clicked source through the native ID-only action instead of webview navigation", async () => {
    render(<InvoiceEligibilityEvidence assessment={assessment} locale="sv" />)
    const source = evidence.sourceReferences[0]
    const link = screen.getByRole("link", { name: source.pinpoint })
    const click = new MouseEvent("click", { bubbles: true, cancelable: true })
    fireEvent(link, click)
    expect(click.defaultPrevented).toBe(true)
    await waitFor(() => expect(invoiceValidationSourceOpen).toHaveBeenCalledWith({
      validationId: evidence.validationId, sourceId: source.id,
    }))
    expect(invoiceValidationSourceOpen).toHaveBeenCalledTimes(1)
  })

  it("shows a source-opening failure without making an accounting mutation", async () => {
    invoiceValidationSourceOpen.mockRejectedValueOnce(new Error("native launch failed"))
    render(<InvoiceEligibilityEvidence assessment={assessment} locale="en" />)
    fireEvent.click(screen.getByRole("link", { name: evidence.sourceReferences[0].pinpoint }))
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not open the source in the system browser")
    expect(screen.getByText(/retained-evidence-digest/)).toBeInTheDocument()
  })

  it("does not substitute this case's sources for an unknown retained validation", () => {
    render(<InvoiceEligibilityEvidence assessment={{ ...assessment, validationId: "another-case" }} locale="en" />)
    expect(screen.queryByRole("region", { name: "Validation sources" })).not.toBeInTheDocument()
    expect(screen.getByText(/another-case/)).toBeInTheDocument()
    expect(invoiceValidationSourceOpen).not.toHaveBeenCalled()
  })
})
