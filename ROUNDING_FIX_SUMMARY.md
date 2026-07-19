# Rounding Fix Summary — Issue #21 (Multi-Asset Pool Share Calculation)

## The real defect (in scope, fixed)

`contracts/contrib/multi_asset_pool.rs` performs the basket pro-rata withdrawal
in `withdraw()` with a naive per-asset truncating division:

```rust
// BEFORE — value is destroyed on every basket withdrawal
for (asset, bal) in pool.balances.iter_mut() {
    let share = (*bal as i128 * amount) / total_value;
    *bal -= share;
}
```

Because each asset's `share` is truncated independently toward zero, the sum of
all shares is **strictly less than `amount`** whenever any remainder exists. The
unaccounted remainder is silently dropped from the pool — exactly the
integer-division asymmetry the issue describes (exploitable as a value-extraction
"rounding tax" across deposit/withdraw cycles).

## The fix

A pure helper `split_pro_rata(weights, total_weight, amount)` computes the
truncated quotient per bucket, then distributes the leftover
(`amount - Σ truncated`) to the buckets with the largest remainders, one unit at
a time, until the sum of shares equals `amount` **exactly**. The `withdraw()`
basket path now calls this helper, so:

- `Σ shares == amount` for every composition (value-conserving),
- deposit(A) → withdraw(A) returns the full deposited amount (symmetry restored),
- no new features, crypto, or architectural refactors were added.

## Verification

`cargo test` / `cargo build` **cannot run** for this file: `multi_asset_pool.rs`
is **not wired into any crate** (`grep` of all `Cargo.toml` finds no reference),
and the build environment has **no Soroban/`wasm32v1-none` target** (rustup has no
default toolchain; only a bare `rustc 1.96.0` is present). The fix was therefore
verified with a standalone `rustc` compile+run of the pure math (keyed by `String`
instead of `soroban_sdk::Address` — identical integer arithmetic). All assertions
passed:

```
ALL SPLIT_PRO_RATA ASSERTIONS PASSED
```

Covered: 2-asset truncation (naive loses 1 unit → fixed conserves 333),
post-deposit withdrawal conservation, 3-asset full withdrawal, single-asset,
zero-total-weight safe-guard, and a 5-asset composition (no negatives, exact sum).
This is AD-HOC evidence, not a wired CI suite — the repo ships no test harness for
this orphan file, so final CI is run by maintainers on merge.

## Out-of-scope items from the issue blueprint (documented, NOT implemented)

The issue's "Implementation Blueprint" assumes a richer architecture than the
code actually contains. The following named items do **not correspond to any
existing code** in this repo and would require inventing crates/state that do not
exist (scope creep, explicitly avoided per the build guardrails):

1. `round_half_even(numerator, denominator)` banker's-rounding helper — the live
   code does no share rounding at all; the truncation asymmetry is fixed by exact
   remainder distribution, which is strictly better than banker's rounding for
   conservation.
2. `cumulative_precision_loss` running counter + `PrecisionLossThresholdExceeded`
   + deposit-pause — there is no share ledger or `total_shares` state to host this.
3. Cross-multiplication with `SIMULATION_PRECISION = 1e12` — no `_calculate_shares`
   / `_calculate_withdrawal` functions exist; shares are computed inline.
4. Deposit-withdraw symmetry fuzz test + numerical unit tests — cannot be wired
   without a Soroban test crate for this orphan file; the standalone math verify
   above covers the symmetry property instead.
5. `ROUNDING_FUZZ_SUMMARY.md` narrative — this file supersedes it with the actual
   fix scope.

These remain valid future enhancements for the maintainers once the pool has a
real share-ledger architecture.

## Files changed

- `contracts/contrib/multi_asset_pool.rs` — `split_pro_rata` helper + fixed
  `withdraw()` basket path. (Only file edited.)
