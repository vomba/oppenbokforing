import type { MessageKey } from "../i18n"

/** Workbench help topics keyed by surface — M8.4 topic registry. */
export const helpTopics = {
  ledger: {
    title: "ledger.overview" satisfies MessageKey,
    help: "help.ledger.overview" satisfies MessageKey,
  },
  documents: {
    title: "documents.title" satisfies MessageKey,
    help: "help.documents.inbox" satisfies MessageKey,
  },
  yearEnd: {
    title: "yearEnd.packageStatus" satisfies MessageKey,
    help: "help.yearEnd.package" satisfies MessageKey,
  },
  invoices: {
    title: "invoices.title" satisfies MessageKey,
    help: "help.invoices.workflow" satisfies MessageKey,
  },
  vat: {
    title: "vat.title" satisfies MessageKey,
    help: "help.vat.returns" satisfies MessageKey,
  },
} as const
