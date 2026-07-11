import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom"
import { WorkspaceProvider } from "./context/WorkspaceContext"
import { LocaleProvider } from "./context/LocaleContext"
import { SimpleModeProvider } from "./context/SimpleModeContext"
import { WorkspaceLocaleHydrator } from "./context/WorkspaceLocaleHydrator"
import { DashboardPage } from "./pages/DashboardPage"
import { OnboardingPage } from "./pages/OnboardingPage"
import { WorkspacePickerPage } from "./pages/WorkspacePickerPage"
import { InvoicesPage } from "./pages/InvoicesPage"
import { DocumentsPage } from "./pages/DocumentsPage"
import { VatPage } from "./pages/VatPage"
import { YearEndPage } from "./pages/YearEndPage"
import { LedgerPage } from "./pages/LedgerPage"
import { SettingsPage } from "./pages/SettingsPage"

export function AppRouter() {
  return (
    <WorkspaceProvider>
      <LocaleProvider>
        <SimpleModeProvider>
          <WorkspaceLocaleHydrator />
          <BrowserRouter>
          <Routes>
            <Route path="/" element={<WorkspacePickerPage />} />
            <Route path="/onboarding" element={<OnboardingPage />} />
            <Route path="/dashboard" element={<DashboardPage />} />
            <Route path="/invoices" element={<InvoicesPage />} />
            <Route path="/ledger" element={<LedgerPage />} />
            <Route path="/documents" element={<DocumentsPage />} />
            <Route path="/vat" element={<VatPage />} />
            <Route path="/year-end" element={<YearEndPage />} />
            <Route path="/settings" element={<SettingsPage />} />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Routes>
        </BrowserRouter>
        </SimpleModeProvider>
      </LocaleProvider>
    </WorkspaceProvider>
  )
}

export default AppRouter
