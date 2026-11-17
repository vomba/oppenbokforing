import invoiceV1 from "../../fixtures/accounting-validation/2026-domestic-service25-exact.json"
import invoiceV2 from "../../fixtures/accounting-validation/2026-domestic-service25-exact-v2.json"
import cashV1 from "../../fixtures/accounting-validation/2026-cash-pdf-full.json"

type Source = Readonly<{ id: string; url: string; label: string }>
const invoiceSources = Object.freeze(invoiceV1.sourceReferences.map(source => Object.freeze({ id: source.id, url: source.url, label: source.pinpoint })))
// Presentation only. Native code owns manifest byte digests and live activation.
// Never hash JSON parsed numbers to reconstruct the source evidence identity.
const registry: Readonly<Record<string, readonly Source[]>> = Object.freeze({
  [invoiceV1.validationId]: invoiceSources,
  [invoiceV2.validationId]: invoiceSources,
  [cashV1.validationId]: Object.freeze([
    ...cashV1.sourceReferences.map(source => Object.freeze({ id: source.id, url: source.url, label: source.pinpoint })),
    ...cashV1.clockPolicy.sourceReferences.map(source => Object.freeze({ id: source.id, url: source.url, label: source.title })),
  ]),
})
export function validationSources(validationId: string): readonly Source[] {
  return Object.hasOwn(registry, validationId) ? registry[validationId] : []
}
