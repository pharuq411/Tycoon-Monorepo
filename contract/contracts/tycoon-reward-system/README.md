# tycoon-reward-system

A Soroban smart contract that manages redeemable vouchers for the **Tycoon** platform.
Players earn vouchers through gameplay; each voucher locks a TYC amount that the holder
can redeem at any time to receive the corresponding TYC tokens.

## Features

| Feature | Description |
|---|---|
| Voucher minting | Admin or a designated backend minter creates vouchers for players |
| Voucher redemption | Holders burn a voucher to receive the locked TYC |
| Token transfer | Vouchers are transferable between addresses |
| Emergency pause | Admin can halt redemptions and transfers instantly |
| Admin withdrawal | Admin can withdraw TYC / USDC held by the contract |
| Backend minter | Optional off-chain service address with mint-only rights |
| Balance queries | `get_balance`, `owned_token_count`, `get_voucher_value` |

## Token ID Ranges

| Range start | Purpose |
|---|---|
| `1 000 000 000` | Redeemable vouchers (`VOUCHER_ID_START`) |

## Project Structure

```
tycoon-reward-system/
├── src/
│   ├── lib.rs      # Contract logic, storage, internal helpers
│   └── test.rs     # Unit & integration tests (30+ cases)
├── Cargo.toml
└── README.md
```

## Prerequisites

- Rust toolchain (stable, `wasm32-unknown-unknown` target)
- [Stellar CLI](https://github.com/stellar/stellar-cli)

```bash
rustup target add wasm32-unknown-unknown
cargo install --locked stellar-cli
```

## Building

```bash
# From the workspace root (contract/)
cargo build --target wasm32-unknown-unknown --release

# Or with the Stellar CLI
stellar contract build
```

## Testing

```bash
# From the workspace root or this directory
cargo test --all

# With printed output
cargo test -- --nocapture

# Run a single test
cargo test test_voucher_flow
```

## Function Reference

### `initialize(admin, tyc_token, usdc_token)`

Sets up the contract.  Must be called exactly once.

| Parameter | Type | Description |
|---|---|---|
| `admin` | `Address` | Contract administrator |
| `tyc_token` | `Address` | TYC fungible-token contract |
| `usdc_token` | `Address` | USDC token contract (used in withdrawal allow-list) |

**Panics**: `"Already initialized"` on repeat calls.

---

### `mint_voucher(caller, to, tyc_value) -> u128`

Mints a new voucher worth `tyc_value` TYC for `to`.

| Parameter | Type | Description |
|---|---|---|
| `caller` | `Address` | Admin or backend minter — must sign |
| `to` | `Address` | Voucher recipient |
| `tyc_value` | `u128` | TYC amount locked in the voucher (> 0) |

**Returns**: the new token ID.

**Panics**:
- `"Voucher value must be greater than zero"`
- `"Unauthorized: only admin or backend minter can mint"`

**Event emitted**: `("V_Mint", to, token_id) → tyc_value`

---

### `redeem_voucher_from(redeemer, token_id)`

Burns the voucher and transfers the locked TYC to `redeemer`.

**Panics**:
- `"Contract is paused"`
- `"Invalid token_id"` — voucher not found / already redeemed
- `"Insufficient balance"` — redeemer does not hold this voucher

**Event emitted**: `("Redeem", redeemer, token_id) → tyc_value`

---

### `transfer(from, to, token_id, amount)`

Transfers voucher tokens between addresses.

**Panics**:
- `"Contract is paused"`
- `"Transfer amount must be greater than zero"`
- `"Insufficient balance"`

**Event emitted**: `("Transfer", from, to, token_id) → amount`

---

### `withdraw_funds(token, to, amount)`

Admin-only.  Moves tokens held by the contract to `to`.

| Parameter | Type | Description |
|---|---|---|
| `token` | `Address` | Must be TYC or USDC |
| `to` | `Address` | Recipient |
| `amount` | `u128` | Amount to send (> 0) |

**Panics**:
- `"Withdrawal amount must be greater than zero"`
- `"Invalid token: not in allowlist"`
- `"Insufficient contract balance"`

**Event emitted**: `("Withdraw", token, to) → amount`

---

### `pause()` / `unpause()`

Admin-only.  Toggle the emergency pause flag.

**Events emitted**: `("Paused",) → true` / `("Unpaused",) → false`

---

### `set_backend_minter(admin, new_minter)` / `clear_backend_minter(admin)`

Admin-only.  Designate or remove an off-chain service address that may call
`mint_voucher`.

---

### `get_backend_minter() -> Option<Address>`

Returns the current backend minter, or `None` if unset.

---

### `get_balance(owner, token_id) -> u64`

Returns the token balance for a specific owner/token pair.  Returns `0` for unknown pairs.

---

### `get_voucher_value(token_id) -> Option<u128>`

Returns the locked TYC amount for a voucher, or `None` if not found / already redeemed.

---

### `owned_token_count(owner) -> u32`

Returns the number of distinct voucher types currently held by `owner`.

---

### `is_paused() -> bool`

Returns `true` when the emergency pause is active.

---

## Storage Layout

| Key | Storage tier | Description |
|---|---|---|
| `DataKey::Admin` | persistent | Admin address |
| `DataKey::TycToken` | persistent | TYC token address |
| `DataKey::UsdcToken` | persistent | USDC token address |
| `DataKey::VoucherCount` | persistent | Next voucher ID counter |
| `DataKey::Paused` | persistent | Pause flag |
| `DataKey::BackendMinter` | persistent | Optional minter address |
| `DataKey::Balance(addr, id)` | persistent | Per-address, per-token balance |
| `DataKey::VoucherValue(id)` | persistent | Locked TYC per voucher |
| `DataKey::OwnedTokenCount(addr)` | persistent | Distinct token types per address |

## Events

| Symbol | Topics | Data | Trigger |
|---|---|---|---|
| `Mint` | `(to, token_id)` | `amount` | `_mint` internal |
| `Burn` | `(from, token_id)` | `amount` | `_burn` internal |
| `V_Mint` | `(to, token_id)` | `tyc_value` | `mint_voucher` |
| `Redeem` | `(redeemer, token_id)` | `tyc_value` | `redeem_voucher_from` |
| `Transfer` | `(from, to, token_id)` | `amount` | `transfer` |
| `Withdraw` | `(token, to)` | `amount` | `withdraw_funds` |
| `Paused` | none | `true` | `pause` |
| `Unpaused` | none | `false` | `unpause` |
| `set_min` | `(new_minter)` | `()` | `set_backend_minter` |
| `clr_min` | none | `()` | `clear_backend_minter` |

## Deployment (Testnet Example)

```bash
# Deploy
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/tycoon_reward_system.wasm \
  --source <DEPLOYER_SECRET> \
  --network testnet

# Initialise
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_SECRET> \
  --network testnet \
  -- initialize \
  --admin <ADMIN_ADDRESS> \
  --tyc_token <TYC_CONTRACT_ID> \
  --usdc_token <USDC_CONTRACT_ID>

# Mint a voucher
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_SECRET> \
  --network testnet \
  -- mint_voucher \
  --caller <ADMIN_ADDRESS> \
  --to <PLAYER_ADDRESS> \
  --tyc_value 500

# Redeem
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <PLAYER_SECRET> \
  --network testnet \
  -- redeem_voucher_from \
  --redeemer <PLAYER_ADDRESS> \
  --token_id <TOKEN_ID>
```

## Security

- **Admin authorization** is enforced by Soroban's `require_auth()` on every privileged call.
- **Reentrancy** is implicitly prevented by Soroban's execution model; no recursive calls can occur within a single transaction.
- **Token allow-list** on `withdraw_funds` prevents draining arbitrary tokens.
- **Zero-value guards** on `mint_voucher`, `transfer`, and `withdraw_funds` reject economically invalid inputs.
- **Stale-state safety**: `get_balance`, `get_voucher_value`, and `owned_token_count` all return safe defaults (`0` / `None`) for unknown keys — they never panic on missing storage.

## Development

```bash
# Format
cargo fmt --all

# Lint
cargo clippy -- -D warnings
```

## License

[License Information Here]
