# VeriFi AI

Wallet-native finance with optional on-chain loans. This repo is a **10–15 minute Rust hiring screen**.

## Your task

`VeriFiLoan` can create / approve / disburse a loan, but it cannot record **partial repayments**. Finish that in `loan/src/lib.rs`.

Do not edit the tests.

```bash
cd loan
cargo test
```

You should see failing tests under `record_payment` / `remaining_of`. Make them pass.

### Implement

1. **`record_payment(caller, loan_id, amount_cents)`**
   - Platform **or** the borrower may call it
   - Loan must be `Disbursed` (otherwise `InvalidStatusTransition`)
   - `amount_cents` must be `> 0` and `<=` remaining (`InvalidAmount` otherwise)
   - Decrease remaining
   - If remaining hits 0, set status to `Repaid` and push `LoanStatusUpdated`
   - Push `PaymentRecorded { loan_id, payer, amount_cents, remaining_cents }` (`payer` is `caller`)

2. **`remaining_of(loan_id) -> u64`**
   - Return `LoanNotFound` if the loan does not exist

3. **Fix `mark_defaulted`**
   - It currently defaults an `Approved` loan. Only `Disbursed` is allowed.
   - Anything else (including `Repaid`) must return `InvalidStatusTransition`

Use the `LoanError` variants already on the type. Do not change function signatures, `Event`, or the `Loan` struct.

## Submit

Work on your own branch and commit locally. Do not push. Confirmation is a frontend simulation.

```bash
git checkout -b <your-name>/verifi-loan
git add loan/src/lib.rs
git commit -m "Record partial loan repayments"
```

## Optional: run the app

```bash
cp .env.example .env
npm install
npm run db:push
npm run db:seed
npm run dev
```

App: http://localhost:5173 · API: http://localhost:3001
