import { Link } from "react-router-dom"

export function VoucherTraceLink({
  voucherId,
  label,
}: {
  voucherId: string
  label?: string
}) {
  return (
    <Link to={`/ledger?voucherId=${encodeURIComponent(voucherId)}`}>
      {label ?? "Open ledger trace"}
    </Link>
  )
}
