import { fireEvent, render, screen } from "@testing-library/react"
import type { ComponentProps } from "react"
import { describe, expect, it, vi } from "vitest"
import { ActionReviewDialog } from "./ActionReviewDialog"

function renderDialog(overrides: Partial<ComponentProps<typeof ActionReviewDialog>> = {}) {
  const onConfirm = vi.fn()
  const onCancel = vi.fn()
  render(
    <ActionReviewDialog
      open
      title="Review invoice issue"
      summary="Customer AB: SEK 1,250 including VAT."
      consequences={["The invoice becomes issued and can only be corrected with a credit invoice."]}
      correction="Create a credit invoice if the issued invoice is wrong."
      confirmLabel="Issue invoice"
      cancelLabel="Cancel"
      busy={false}
      onConfirm={onConfirm}
      onCancel={onCancel}
      {...overrides}
    />,
  )
  return { onConfirm, onCancel }
}

describe("ActionReviewDialog", () => {
  it("focuses Cancel and exposes facts and correction in an accessible modal", () => {
    renderDialog()

    const dialog = screen.getByRole("dialog", { name: "Review invoice issue" })
    expect(dialog).toHaveTextContent("Customer AB: SEK 1,250 including VAT.")
    expect(dialog).toHaveTextContent("Create a credit invoice if the issued invoice is wrong.")
    expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus()
  })

  it("does not invoke confirm when Cancel is chosen", () => {
    const { onConfirm, onCancel } = renderDialog()

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }))

    expect(onCancel).toHaveBeenCalledOnce()
    expect(onConfirm).not.toHaveBeenCalled()
  })

  it("disables both outcomes while caller work is busy", () => {
    renderDialog({ busy: true })

    expect(screen.getByRole("button", { name: "Cancel" })).toBeDisabled()
    expect(screen.getByRole("button", { name: "Issue invoice" })).toBeDisabled()
  })

  it("traps keyboard focus between Cancel and confirm", () => {
    renderDialog()

    const cancel = screen.getByRole("button", { name: "Cancel" })
    const confirm = screen.getByRole("button", { name: "Issue invoice" })
    fireEvent.keyDown(document, { key: "Tab", shiftKey: true })
    expect(confirm).toHaveFocus()

    fireEvent.keyDown(document, { key: "Tab" })
    expect(cancel).toHaveFocus()
  })
})
