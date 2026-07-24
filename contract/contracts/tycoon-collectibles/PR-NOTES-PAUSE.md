# feat(tycoon-collectibles): pause flag — mutations panic when paused [SW-CT-PAUSE-001]

## Summary

Implements the `pause` flag for the `tycoon-collectibles` contract so that **all mutation entrypoints return `ContractPaused` (error 9) when the contract is paused**. Read-only queries are unaffected.

## What Changed

### `src/lib.rs`
- Added `require_not_paused(env)` helper — single source of truth for the pause check.
- Wired the guard into every mutation entrypoint:
  - `stock_shop`
  - `restock_collectible`
  - `buy_collectible_from_shop`
  - `buy_collectible`
  - `transfer`
  - `burn`
  - `burn_collectible_for_perk` (replaced inline check with shared helper)
  - `backend_mint`
  - `mint_collectible`
- Registered new `pause_tests` module.

### `src/pause_tests.rs` (new)
18 focused tests covering:
- Each mutation entrypoint returns `ContractPaused` when paused
- State is unchanged after a blocked call (no partial writes)
- Operations succeed after unpause
- Read-only queries (`balance_of`, `get_stock`, `get_token_perk`, `owned_token_count`, `is_contract_paused`) work while paused
- Pause is idempotent (set_pause(true) twice, set_pause(false) twice)

## API Contract

| Entrypoint | Paused behaviour |
|---|---|
| `set_pause` | Always callable by admin (how you unpause) |
| `is_contract_paused` | Always readable |
| `balance_of`, `get_stock`, `get_token_perk`, `tokens_of`, `owned_token_count` | Always readable |
| All other mutations | Returns `ContractPaused` (error 9) |

## Testing

```bash
cd contract
cargo test -p tycoon-collectibles
```

All existing tests continue to pass. New `pause_tests` module adds 18 tests.

## Rollout

1. Deploy updated WASM.
2. Admin calls `set_pause(true)` to halt minting/trading during incidents.
3. Admin calls `set_pause(false)` to resume.

Closes SW-CT-PAUSE-001
