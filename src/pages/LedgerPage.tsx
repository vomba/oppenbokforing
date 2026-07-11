import { useEffect, useState } from "react"
import { useSearchParams } from "react-router-dom"
import { AppSidebar } from "../components/AppSidebar"
import { HelpTip } from "../components/HelpTip"
import { useWorkspace } from "../context/WorkspaceContext"
import { useLocale } from "../context/LocaleContext"
import { t } from "../i18n"
import { helpTopics } from "../lib/helpTopics"
import {
  accountList,
  appErrorMessage,
  fiscalPeriodList,
  voucherCount,
  voucherGet,
  voucherList,
  type AccountSummary,
  type FiscalPeriodSummary,
  type VoucherDetail,
  type VoucherSummary,
} from "../lib/commands"

function formatSek(minor: number) {
  return `${(minor / 100).toLocaleString("sv-SE", { minimumFractionDigits: 2 })} kr`
}

export function LedgerPage() {
  const { workspace } = useWorkspace()
  const { locale } = useLocale()
  const [searchParams] = useSearchParams()
  const [vouchers, setVouchers] = useState<VoucherSummary[]>([])
  const [postedVoucherCount, setPostedVoucherCount] = useState(0)
  const [accounts, setAccounts] = useState<AccountSummary[]>([])
  const [periods, setPeriods] = useState<FiscalPeriodSummary[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [detail, setDetail] = useState<VoucherDetail | null>(null)
  const [status, setStatus] = useState(t(locale, "ledger.status"))
  const [busy, setBusy] = useState(false)

  const voucherFromUrl = searchParams.get("voucherId")

  async function refresh() {
    if (!workspace) return
    const [voucherRows, accountRows, periodRows, postedCount] = await Promise.all([
      voucherList({ status: "posted", sourceType: null, limit: 100, beforeId: null }),
      accountList(),
      fiscalPeriodList(),
      voucherCount({ status: "posted" }),
    ])
    setVouchers(voucherRows)
    setPostedVoucherCount(postedCount)
    setAccounts(accountRows)
    setPeriods(periodRows)
  }

  useEffect(() => {
    if (!workspace) return
    refresh()
      .then(() => setStatus(t(locale, "ledger.status")))
      .catch(() => setStatus(t(locale, "ledger.loadFailed")))
  }, [workspace, locale])

  useEffect(() => {
    if (voucherFromUrl) {
      setSelectedId(voucherFromUrl)
    }
  }, [voucherFromUrl])

  useEffect(() => {
    if (!workspace || !selectedId) {
      setDetail(null)
      return
    }
    setBusy(true)
    voucherGet({ voucherId: selectedId })
      .then((row) => {
        setDetail(row)
        setStatus(t(locale, "ledger.detailLoaded"))
      })
      .catch((error) => {
        setDetail(null)
        setStatus(appErrorMessage(error, t(locale, "ledger.detailFailed")))
      })
      .finally(() => setBusy(false))
  }, [workspace, selectedId, locale])

  return (
    <main className="app-shell">
      <AppSidebar current="ledger" />

      <section className="workspace">
        <header className="topbar">
          <div>
            <p className="eyebrow">{workspace?.name ?? "—"}</p>
            <h2>
              {t(locale, "ledger.title")}
              <HelpTip label={t(locale, helpTopics.ledger.title)}>
                {t(locale, helpTopics.ledger.help)}
              </HelpTip>
            </h2>
            <p className="status-line" aria-live="polite">
              {status}
            </p>
          </div>
        </header>

        <section className="dashboard-grid" aria-label={t(locale, "ledger.overview")}>
          <article className="metric metric-neutral">
            <span>{t(locale, "ledger.postedVouchers")}</span>
            <strong>{postedVoucherCount}</strong>
          </article>
          <article className="metric metric-neutral">
            <span>{t(locale, "ledger.accounts")}</span>
            <strong>{accounts.length}</strong>
          </article>
          <article className="metric metric-neutral">
            <span>{t(locale, "ledger.lockedPeriods")}</span>
            <strong>{periods.filter((row) => row.status === "locked").length}</strong>
          </article>
        </section>

        <section className="workbench">
          <div className="panel">
            <header>
              <p className="eyebrow">{t(locale, "ledger.vouchers")}</p>
              <h3>{t(locale, "ledger.voucherList")}</h3>
            </header>
            <table className="data-table">
              <thead>
                <tr>
                  <th scope="col">{t(locale, "ledger.date")}</th>
                  <th scope="col">{t(locale, "ledger.source")}</th>
                  <th scope="col">{t(locale, "ledger.statusLabel")}</th>
                  <th scope="col">{t(locale, "ledger.amount")}</th>
                </tr>
              </thead>
              <tbody>
                {vouchers.map((voucher) => (
                  <tr
                    key={voucher.id}
                    className={selectedId === voucher.id ? "selected-row" : undefined}
                  >
                    <td>
                      <button
                        type="button"
                        className="link-button"
                        disabled={busy}
                        aria-pressed={selectedId === voucher.id}
                        onClick={() => setSelectedId(voucher.id)}
                      >
                        {voucher.accountingDate ?? voucher.postedAt ?? "—"}
                      </button>
                    </td>
                    <td>{voucher.sourceType}</td>
                    <td>{voucher.status}</td>
                    <td>{formatSek(voucher.debitTotalMinor)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          {detail ? (
            <div className="panel">
              <header>
                <p className="eyebrow">{t(locale, "ledger.voucherDetail")}</p>
                <h3>{detail.sourceType}</h3>
              </header>
              <p className="status-line">
                {detail.status} · {detail.accountingDate ?? detail.postedAt ?? "—"}
              </p>
              <table className="data-table">
                <thead>
                  <tr>
                    <th scope="col">{t(locale, "ledger.account")}</th>
                    <th scope="col">{t(locale, "ledger.debit")}</th>
                    <th scope="col">{t(locale, "ledger.credit")}</th>
                    <th scope="col">{t(locale, "ledger.vatCode")}</th>
                  </tr>
                </thead>
                <tbody>
                  {detail.lines.map((line) => (
                    <tr key={`${line.accountNumber}-${line.debitMinor}-${line.creditMinor}`}>
                      <td>
                        {line.accountNumber} {line.accountName}
                      </td>
                      <td>{line.debitMinor > 0 ? formatSek(line.debitMinor) : "—"}</td>
                      <td>{line.creditMinor > 0 ? formatSek(line.creditMinor) : "—"}</td>
                      <td>{line.vatCode ?? "—"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : null}

          <div className="panel">
            <header>
              <p className="eyebrow">{t(locale, "ledger.accounts")}</p>
              <h3>{t(locale, "ledger.accountBalances")}</h3>
            </header>
            <table className="data-table">
              <thead>
                <tr>
                  <th scope="col">{t(locale, "ledger.account")}</th>
                  <th scope="col">{t(locale, "ledger.balance")}</th>
                </tr>
              </thead>
              <tbody>
                {accounts.map((account) => (
                  <tr key={account.id}>
                    <td>
                      {account.number} {account.name}
                    </td>
                    <td>{formatSek(account.balanceMinor)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          <div className="panel">
            <header>
              <p className="eyebrow">{t(locale, "ledger.periodLocks")}</p>
              <h3>{t(locale, "ledger.fiscalPeriods")}</h3>
            </header>
            <table className="data-table">
              <thead>
                <tr>
                  <th scope="col">{t(locale, "ledger.period")}</th>
                  <th scope="col">{t(locale, "ledger.statusLabel")}</th>
                </tr>
              </thead>
              <tbody>
                {periods.map((period) => (
                  <tr key={period.id}>
                    <td>{period.periodKey}</td>
                    <td>{period.status}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>
      </section>
    </main>
  )
}
