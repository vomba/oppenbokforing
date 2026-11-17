import { expect, it } from "vitest"
import v1 from "../../fixtures/accounting-validation/2026-domestic-service25-exact.json"
import v2 from "../../fixtures/accounting-validation/2026-domestic-service25-exact-v2.json"
import cash from "../../fixtures/accounting-validation/2026-cash-pdf-full.json"
import { validationSources } from "./validationRegistry"
it("preserves historical invoice URLs, supports v2 inheritance and all cash plus clock source IDs", () => {
  expect(validationSources(v1.validationId)).toEqual(v1.sourceReferences.map(s => ({ id: s.id, url: s.url, label: s.pinpoint })))
  expect(validationSources(v2.validationId)).toEqual(validationSources(v1.validationId))
  expect(validationSources(cash.validationId)).toHaveLength(12)
  expect(validationSources(cash.validationId).map(s => s.id)).toEqual([...cash.sourceReferences, ...cash.clockPolicy.sourceReferences].map(s => s.id))
  expect(validationSources("unknown")).toEqual([])
})
