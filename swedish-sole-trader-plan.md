# Swedish Sole-Trader Taxes and Accounting Desktop App

## Summary

Build a Swedish `enskild firma` desktop app that helps with invoicing, bookkeeping, cash and tax planning, VAT handling, and year-end tax preparation. The first version should optimize for a solo freelancer who may also have salary income, so `FA-skatt` support is required. It should work local-first, store accounting data on the user's machine, and produce filing-ready exports rather than directly submitting to Skatteverket.

## Core Scope

- Local desktop onboarding for sole traders with business income and optional employment income.
- Invoicing with correct VAT, no-VAT, and F-tax wording.
- Double-entry bookkeeping with immutable vouchers and local source attachments.
- Cash planning with reserved VAT and reserved tax estimates.
- VAT tracking for registered, exempt, and voluntary-registration cases.
- Year-end support for simplified annual accounts and `NE-bilaga` preparation.
- Local backups and export packages for accountant or authority workflows.

## Desktop Product Defaults

- Build as a Tauri v2 desktop app with a React/Vite TypeScript renderer and Rust command layer.
- Store app data in a local SQLite database in the OS app data directory.
- Store receipts, invoice PDFs, and export bundles as local content-addressed files.
- Use durable in-process background jobs instead of Redis or a hosted queue.
- Start with Swedish-resident sole traders only.
- Support `FA-skatt` from day one.
- Support manual entry and CSV import first; bank sync later.
- Support filing-ready exports and guided workflows first, not direct submission.
- Use `K1` simplified annual accounts as the first accounting baseline.

## Must-Have Rules

- A sole trader with salary income needs `FA-skatt`, not only `F-skatt`.
- VAT-exempt businesses under the threshold must not charge VAT.
- VAT-registered businesses must file VAT returns even when the return is zero.
- Sole traders file annual income tax using `INK1` with the `NE` appendix.
- Accounting records must be continuous, auditable, and retained for seven years.

## Desktop Validation Targets

- The app runs offline after installation and can open an existing local workspace without network access.
- A user with salary plus sole-trader income can see separate tax planning for each.
- A VAT-exempt user can invoice without VAT and get warned before the threshold.
- A VAT-registered user can prepare a compliant VAT return from local ledger data.
- A user can close the year and produce reviewable annual accounts and an `NE` draft.
- A user can create an encrypted backup or export package from local data and evidence files.

## Sources Used

- Tauri v2 architecture documentation: https://v2.tauri.app/concept/architecture/
- Tauri v2 SQL plugin documentation: https://v2.tauri.app/plugin/sql/
- Tauri v2 file-system plugin documentation: https://v2.tauri.app/plugin/file-system/
- Skatteverket F-tax / FA-tax guidance: https://www.skatteverket.se/servicelankar/otherlanguages/englishengelska/businessesandemployers/startingandrunningaswedishbusiness/registeringabusiness/approvalforftax.4.676f4884175c97df4192308.html
- Skatteverket VAT registration guidance: https://www.skatteverket.se/servicelankar/otherlanguages/englishengelska/businessesandemployers/startingandrunningaswedishbusiness/registeringabusiness/registeryourbusinessforvat.4.6e1dd38d196873bc1e1376.html
- Skatteverket VAT exemption guidance: https://www.skatteverket.se/servicelankar/otherlanguages/englishengelska/businessesandemployers/startingandrunningaswedishbusiness/registeringabusiness/incertaincasesyoudonotneedtoregisteryourbusinessforvat.4.6e1dd38d196873bc1e1cff.html
- Skatteverket income tax for sole traders: https://www.skatteverket.se/servicelankar/otherlanguages/englishengelska/businessesandemployers/startingandrunningaswedishbusiness/declaringtaxesbusinesses/incometax/incometaxreturnsforsoletraders.4.676f4884175c97df41913f3.html
- Skatteverket VAT declarations: https://www.skatteverket.se/servicelankar/otherlanguages/englishengelska/businessesandemployers/startingandrunningaswedishbusiness/declaringtaxesbusinesses/vatdeclarations.4.12815e4f14a62bc048f52be.html
- Bokföringsnämnden enskilda näringsidkare: https://www.bfn.se/redovisningsregler/vad-galler-for/enskilda-naringsidkare/
