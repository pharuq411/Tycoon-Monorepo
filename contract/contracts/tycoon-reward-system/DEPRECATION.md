# Deprecation Path — Legacy Entrypoints

## Overview

This document describes the controlled deprecation path introduced in
**contract v2** for legacy entrypoints that existed in v1 of the
`tycoon-reward-system` contract.

---

## Deprecated Entrypoints

### `redeem_voucher(token_id: u128)`

| Attribute | Value |
|-----------|-------|
| Status | **Deprecated** (v2) |
| Removal target | Next major version |
| Replacement | `redeem_voucher_from(redeemer, token_id)` |
| Monitoring key | `get_deprecated_call_count(0)` |

#### Why it was deprecated

The original `redeem_voucher` did not require an explicit `redeemer`
address parameter. This created an implicit dependency on the transaction
source account, making the contract surface harder to audit and the
redemption intent ambiguous when called through proxy contracts.

`redeem_voucher_from` requires the redeemer to pass their address
explicitly and to provide a Soroban authorization signature, closing the
ambiguity and aligning with the rest of the contract's authorization model.

#### Runtime behaviour in v2

Calling `redeem_voucher` in v2 will:

1. **Emit** a `DeprecWarn` event with topic `("DeprecWarn", "rdm_v")` and
   data `"rdm_vf"`, visible to indexers and monitoring dashboards.
2. **Increment** the on-chain deprecated-call counter at
   `DataKey::DeprecatedCallCount(0)`, which persists across calls.
3. **Panic** immediately with the message:
   ```
   DEPRECATED: use redeem_voucher_from(redeemer, token_id)
   ```
   No tokens are moved or burned.

#### Migration guide

Replace:

```rust
// v1 — legacy call
client.redeem_voucher(&token_id);
```

With:

```rust
// v2 — canonical call
client.redeem_voucher_from(&redeemer, &token_id);
```

The `redeemer` address must:
- Hold a non-zero balance of `token_id`.
- Provide a valid Soroban authorization for the call.

#### Monitoring migration progress

Query the deprecated-call counter at any time:

```rust
let call_count: u32 = client.get_deprecated_call_count(&0);
```

A count of `0` means no callers are using the legacy entrypoint and it is
safe to remove in the next major deploy.

---

## Contract Version

Use `get_contract_version()` to gate migration logic in off-chain code or
client libraries:

```rust
let version: u32 = client.get_contract_version();
// version == 1  →  only redeem_voucher available
// version == 2  →  redeem_voucher deprecated, redeem_voucher_from is canonical
```

Version history:

| Version | Notable changes |
|---------|-----------------|
| 1 | Initial deployment: `redeem_voucher`, `mint_voucher`, `transfer` |
| 2 | `redeem_voucher` deprecated (emits event + increments counter + panics); `redeem_voucher_from` introduced as canonical replacement; `withdraw_funds`, pause/unpause, and backend-minter management added |

---

## On-Chain Monitoring Events

| Event topic | Meaning |
|-------------|---------|
| `("DeprecWarn", "rdm_v")` | A caller invoked the deprecated `redeem_voucher` entrypoint |

Subscribe to this event in your indexer to alert on remaining legacy
callers and accelerate migration.

---

## Storage Impact

The deprecation path adds one new `DataKey` variant:

```rust
DataKey::DeprecatedCallCount(u32)  // per-entrypoint call counter
```

This key is created on first use and removed only if the contract is
re-initialized (which requires a full redeploy). The storage overhead is
negligible (one `u32` per deprecated entrypoint).

---

## Security Notes

- The deprecated entrypoint **always panics** before touching any token
  balances. There is no path through `redeem_voucher` that can drain
  funds or alter voucher state.
- The deprecation counter uses **saturating addition** so an extreme call
  volume (> 2³²) cannot cause a storage overflow.
- Events emitted before the panic are still visible on-chain and in
  transaction metadata, enabling forensic analysis.

---

## Removal Checklist

Before removing `redeem_voucher` in a future major version:

- [ ] `get_deprecated_call_count(0)` returns `0` across a representative
      monitoring window.
- [ ] All known front-end clients and SDK integrations have been updated to
      `redeem_voucher_from`.
- [ ] A migration notice has been published in the project changelog and
      communicated to downstream consumers.
- [ ] The removal is reflected in `get_contract_version` (version bump to 3+).
