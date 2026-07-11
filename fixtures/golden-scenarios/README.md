# Golden Scenario Fixtures

These fixtures define behavior that must stay stable as the implementation grows. They are not legal advice; they are engineering fixtures derived from the current product plan and cited rule sources.

Each fixture contains:

- `id`
- `title`
- `profile`
- `transactions`
- `expected`
- `sources`

Tests should load these files and verify the rule engine, ledger engine, VAT engine, and year-end workbench against the expected outcomes.

