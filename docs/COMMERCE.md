# Commerce: spend requests, approval and human checkout

RAVENBOT commerce v1 is a **permission and record-keeping layer**, not a payment
system. An agent may ask to buy something. A person reviews the request, approves
or denies it, opens the merchant's own checkout page in a browser, completes the
purchase, and then records the outcome. RAVENBOT never checks out, never sees card
data, and never claims a purchase happened on its own.

## What ships

- The `spend_request` agent tool creates a `pending` row. It has no purchase
  action, no payment client, no card fields and no network access.
- An owner-only **Settings → Commerce** panel stores the workspace policy,
  lists requests, approves or denies them, opens the checkout URL externally,
  marks a request purchased, or cancels it.
- Limits are enforced when the request is created **and again when it is
  approved**, against the policy in force at that moment.
- The daily limit is checked inside the same SQLite transaction that flips a
  request to `approved`, so two approvals cannot both spend the last of the
  day's budget.
- Decisions and purchases are written to the audit log through
  `ApprovalDecision` entries and stored on the request row.

## Policy

The policy is stored in `app_settings`:

| Key | Meaning | Default |
|---|---|---|
| `commerce_enabled` | Master switch; anything other than `true` fails closed | off |
| `commerce_currency` | One three-letter code for the workspace | `USD` |
| `commerce_per_request_limit_minor` | Maximum for one request, in minor units | empty |
| `commerce_daily_limit_minor` | Maximum approved/purchased per UTC day, in minor units | empty |
| `commerce_merchant_allowlist` | Comma/line separated bare merchant domains | empty |

Amounts are **minor units**: cents for USD/EUR. The UI accepts whole numbers and
labels the field “minor units”; there is no decimal parsing and no currency
conversion. A currency mismatch between a request and the policy is refused.

When commerce is enabled:

- the limits must be greater than zero;
- the per-request limit cannot exceed the daily limit;
- the merchant allowlist must contain at least one domain.

An enabled policy with no merchants is invalid, not “allow everything”. A saved
invalid policy is shown in the panel with its exact validation error so the owner
can repair it.

### Merchant allowlist

Entries are bare hostnames such as `example.com` or `shop.example.com`. Schemes,
ports, paths, usernames and single-label hosts are rejected instead of being
trimmed into a different host. Matching is:

- exact (`shop.example.com`), or
- a subdomain of an entry (`checkout.shop.example.com` when
  `shop.example.com` is listed).

`evil-example.com` and `example.com.evil.test` do **not** match `example.com`.

## The `spend_request` tool

Parameters:

| Field | Rule |
|---|---|
| `title` | One line, 1–200 characters |
| `merchant` | Display name, one line, 1–200 characters |
| `url` | Absolute `https://` URL with a dotted host and no credentials |
| `amount_minor` | Integer, 1–1,000,000,000,000 |
| `currency` | Three-letter code, normalized to upper case |
| `reason` | 1–2,000 characters; the approver sees it verbatim |

The tool reads the saved policy, checks the request against it, counts today's
approved and purchased total, and inserts a `pending` row tied to the bot, thread
and run. It returns:

```json
{
  "request_id": "…",
  "status": "pending",
  "approved": false,
  "purchased": false,
  "amount_minor": 12500,
  "currency": "USD",
  "merchant_host": "shop.example.com",
  "message": "Spend request saved. Nothing was purchased. The workspace owner must approve it in Settings → Commerce."
}
```

The tool is risk `low`, requires no permissions, and has no counterpart named
`purchase`, `pay`, `checkout` or `buy`. A disabled or invalid policy is an error;
the tool does not create a row and does not retry with relaxed limits.

## Request lifecycle

```
pending ──approve──> approved ──mark purchased──> purchased
   │                    │
   ├──deny──> denied    └──cancel──> cancelled
   └──cancel──> cancelled
```

- **pending** — created by the agent, not yet decided. Only pending requests can
  be approved or denied.
- **approved** — budget is reserved for the UTC day; nothing has been bought.
  Only approved requests can be marked purchased.
- **purchased** — the person completed checkout and recorded it. Terminal.
- **denied** — the owner refused the request. Terminal.
- **cancelled** — the owner withdrew a pending or approved request. Terminal.

Every transition is guarded by the current status in SQL. A second click, a
stale panel or a repeated command cannot approve twice or purchase twice.

### Approval-time enforcement

Approval re-reads the saved policy and rebuilds the validated request from the
row. It then re-checks:

1. commerce is enabled;
2. the request currency matches the policy currency;
3. the amount is within the per-request limit;
4. the merchant host is still on the allowlist;
5. today's approved + purchased total plus this request is within the daily
   limit.

Step 5 is repeated inside the approval transaction with the row lock held. A
failure rolls the row back to `pending` and reports the committed total and the
limit. Editing the policy after a request was created therefore applies at
approval time; an old row cannot carry an old, looser policy through.

The daily window is the **UTC calendar day**, based on the approval timestamp.
Approved requests count even if they are never marked purchased. Denied and
cancelled requests do not count.

## Human checkout handoff

“Open checkout” calls the external opener with the stored HTTPS URL. It does not
navigate the app window, submit a form, fill card fields or call a merchant API.
The person completes the purchase in their browser and then chooses “Mark
purchased” (or cancels). “Mark purchased” only records the person's statement; it
does not verify an order.

The request table stores title, merchant, URL, amount, currency, reason, status,
decision metadata and timestamps. It has no card number, CVV, expiry, bank
credential, order id or shipping address column. Do not put card data in the
title or reason.

## Commands and roles

| Command | Purpose | Role |
|---|---|---|
| `get_commerce_settings` | Read policy plus validity | Owner |
| `set_commerce_settings` | Validate and save policy | Owner |
| `list_spend_requests` | List up to 100 requests | Owner |
| `approve_spend_request` | Re-check and approve one pending row | Owner |
| `deny_spend_request` | Deny one pending row, optional note | Owner |
| `mark_spend_purchased` | Record a completed external checkout | Owner |
| `cancel_spend_request` | Cancel a pending or approved row | Owner |

Operators and viewers cannot read or change commerce state. The agent tool is
available to agents, but it can only create pending rows.

## Limits

By design, v1 does **not** have:

- a payment rail, card form, card storage, wallet, bank integration or Stripe;
- automated checkout, headless browser purchase or merchant order API;
- orders, invoices, receipts, refunds, tax, shipping or address handling;
- currency conversion or multiple currencies in one workspace;
- subscriptions, recurring charges or budget rollover;
- auto-approval, agent approval, or approval by an operator/viewer;
- verification that the person actually completed checkout.

These are honest boundaries, not missing wiring. The human checkout step is the
point: RAVENBOT can ask, record and enforce a budget, but it cannot spend.

## Verification

Automated coverage lives in:

- `crates/core/src/commerce.rs` — validation, URL/allowlist boundaries, policy
  evaluation and fail-closed cases;
- `crates/db/src/queries.rs` — creation, terminal transitions and the
  transactional daily-limit check;
- `crates/skills/src/builtin/commerce.rs` — the tool creates only `pending` rows,
  refuses disabled/invalid policies and never purchases;
- `src-tauri/src/access.rs` — every commerce command is owner-only;
- `src/lib/components/CommercePanel.dom.test.ts` — policy saving, approve/deny,
  external checkout, purchase/cancel and invalid-policy display.

The manual check is in [`LIVE-CHECKS.md`](LIVE-CHECKS.md).
