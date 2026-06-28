#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Token-ID range reserved for redeemable vouchers.
pub const VOUCHER_ID_START: u128 = 1_000_000_000;

/// Current contract version — incremented whenever a breaking or significant
/// change is deployed.  Callers can use `get_contract_version` to gate their
/// migration logic.
pub const CONTRACT_VERSION: u32 = 2;

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

/// All persistent storage keys used by the contract.
///
/// - `Balance(owner, token_id)` — token balance for a specific owner/token pair.
/// - `VoucherValue(token_id)` — TYC value locked in a voucher.
/// - `Admin` — the contract administrator address.
/// - `TycToken` — TYC fungible-token contract address.
/// - `UsdcToken` — USDC token contract address.
/// - `VoucherCount` — auto-incrementing voucher ID counter.
/// - `Paused` — emergency-pause flag.
/// - `BackendMinter` — optional off-chain minting key.
/// - `OwnedTokenCount(owner)` — number of distinct token types held.
/// - `DeprecatedCallCount(entrypoint)` — how many times a deprecated
///   entrypoint has been invoked; used for operational monitoring.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// (owner, token_id) → u64 balance
    Balance(Address, u128),
    /// token_id → u128 TYC value
    VoucherValue(u128),
    /// Global: admin address
    Admin,
    /// Global: TYC token contract address
    TycToken,
    /// Global: USDC token contract address
    UsdcToken,
    /// Global: next voucher ID (starts at VOUCHER_ID_START)
    VoucherCount,
    /// Global: emergency pause flag
    Paused,
    /// Global: optional backend minter address
    BackendMinter,
    /// Per-address count of distinct voucher token types held
    OwnedTokenCount(Address),
    /// Deprecated-entrypoint call counter.
    ///
    /// Keyed by a short symbol representing the entrypoint name so that each
    /// deprecated function has an independent counter.  Useful for off-chain
    /// monitoring and deciding when it is safe to fully remove the stub.
    DeprecatedCallCount(u32),
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct TycoonRewardSystem;

#[contractimpl]
impl TycoonRewardSystem {
    // -----------------------------------------------------------------------
    // Initialization
    // -----------------------------------------------------------------------

    /// Initialise the contract.  May only be called once; subsequent calls panic.
    ///
    /// # Arguments
    /// * `admin`     — Address that will own admin privileges.
    /// * `tyc_token` — TYC fungible-token contract.
    /// * `usdc_token`— USDC token contract (used in the withdrawal allow-list).
    ///
    /// # Panics
    /// * `"Already initialized"` — if called more than once.
    pub fn initialize(e: Env, admin: Address, tyc_token: Address, usdc_token: Address) {
        if e.storage().persistent().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        admin.require_auth();
        e.storage().persistent().set(&DataKey::Admin, &admin);
        e.storage().persistent().set(&DataKey::TycToken, &tyc_token);
        e.storage().persistent().set(&DataKey::UsdcToken, &usdc_token);
        e.storage()
            .persistent()
            .set(&DataKey::VoucherCount, &VOUCHER_ID_START);
        e.storage().persistent().set(&DataKey::Paused, &false);
    }

    // -----------------------------------------------------------------------
    // Emergency pause / unpause (admin only)
    // -----------------------------------------------------------------------

    /// Pause the contract — disables voucher redemption and transfers.
    ///
    /// # Panics
    /// * `"Not initialized"` — if called before `initialize`.
    /// * Authorization failure — if caller is not the admin.
    pub fn pause(e: Env) {
        let admin: Address = e
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();
        e.storage().persistent().set(&DataKey::Paused, &true);
        #[allow(deprecated)]
        e.events().publish((symbol_short!("Paused"),), true);
    }

    /// Unpause the contract — re-enables voucher redemption and transfers.
    ///
    /// # Panics
    /// * `"Not initialized"` — if called before `initialize`.
    /// * Authorization failure — if caller is not the admin.
    pub fn unpause(e: Env) {
        let admin: Address = e
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();
        e.storage().persistent().set(&DataKey::Paused, &false);
        #[allow(deprecated)]
        e.events().publish((symbol_short!("Unpaused"),), false);
    }

    // -----------------------------------------------------------------------
    // Backend minter management (admin only)
    // -----------------------------------------------------------------------

    /// Designate an off-chain service address that is allowed to mint vouchers.
    ///
    /// # Arguments
    /// * `admin`       — Must be the current contract admin.
    /// * `new_minter`  — Address to grant minting rights.
    ///
    /// # Panics
    /// * `"Not initialized"` / `"Unauthorized"` — if caller is not admin.
    pub fn set_backend_minter(e: Env, admin: Address, new_minter: Address) {
        let stored_admin: Address = e
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Unauthorized: only admin can set backend minter");
        }
        admin.require_auth();
        e.storage()
            .persistent()
            .set(&DataKey::BackendMinter, &new_minter);
        #[allow(deprecated)]
        e.events()
            .publish((symbol_short!("set_min"), new_minter), ());
    }

    /// Remove the backend minter, reverting to admin-only minting.
    ///
    /// # Panics
    /// * `"Not initialized"` / `"Unauthorized"` — if caller is not admin.
    pub fn clear_backend_minter(e: Env, admin: Address) {
        let stored_admin: Address = e
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Unauthorized: only admin can clear backend minter");
        }
        admin.require_auth();
        e.storage().persistent().remove(&DataKey::BackendMinter);
        #[allow(deprecated)]
        e.events().publish((symbol_short!("clr_min"),), ());
    }

    /// Return the current backend minter address, or `None` if unset.
    pub fn get_backend_minter(e: Env) -> Option<Address> {
        e.storage().persistent().get(&DataKey::BackendMinter)
    }

    // -----------------------------------------------------------------------
    // Voucher minting
    // -----------------------------------------------------------------------

    /// Mint a new redeemable voucher for `to` worth `tyc_value` TYC.
    ///
    /// Callable by the admin or the configured backend minter.
    ///
    /// # Arguments
    /// * `caller`    — Authorised caller (admin or backend minter).
    /// * `to`        — Recipient address.
    /// * `tyc_value` — Amount of TYC the voucher will pay out on redemption.
    ///
    /// # Returns
    /// The new token ID assigned to the voucher.
    ///
    /// # Panics
    /// * `"Not initialized"` — contract not yet set up.
    /// * `"Voucher value must be greater than zero"` — zero-value guard.
    /// * `"Unauthorized: only admin or backend minter can mint"` — wrong caller.
    pub fn mint_voucher(e: Env, caller: Address, to: Address, tyc_value: u128) -> u128 {
        let admin: Address = e
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        caller.require_auth();

        if tyc_value == 0 {
            panic!("Voucher value must be greater than zero");
        }

        let backend_minter: Option<Address> =
            e.storage().persistent().get(&DataKey::BackendMinter);

        let is_admin = caller == admin;
        let is_backend_minter = backend_minter
            .as_ref()
            .map(|m| *m == caller)
            .unwrap_or(false);

        if !is_admin && !is_backend_minter {
            panic!("Unauthorized: only admin or backend minter can mint");
        }

        let mut current_id: u128 = e
            .storage()
            .persistent()
            .get(&DataKey::VoucherCount)
            .unwrap_or(VOUCHER_ID_START);
        let token_id = current_id;
        current_id += 1;
        e.storage()
            .persistent()
            .set(&DataKey::VoucherCount, &current_id);

        e.storage()
            .persistent()
            .set(&DataKey::VoucherValue(token_id), &tyc_value);

        Self::_mint(&e, to.clone(), token_id, 1);

        #[allow(deprecated)]
        e.events()
            .publish((symbol_short!("V_Mint"), to, token_id), tyc_value);

        token_id
    }

    // -----------------------------------------------------------------------
    // Voucher redemption — deprecated legacy entrypoint
    // -----------------------------------------------------------------------

    /// **Deprecated** — use [`redeem_voucher_from`] instead.
    ///
    /// This entrypoint existed in v1 of the contract and accepted only a
    /// `token_id`, implicitly treating the transaction source as the redeemer.
    /// It is retained as a **shim** to avoid breaking callers that have not yet
    /// migrated, but it will be **removed in a future major version**.
    ///
    /// ## Migration guide
    ///
    /// Replace every call of the form:
    /// ```text
    /// redeem_voucher(token_id)
    /// ```
    /// with:
    /// ```text
    /// redeem_voucher_from(redeemer, token_id)
    /// ```
    /// where `redeemer` is the address that holds the voucher and must
    /// authorise the transaction.
    ///
    /// ## Behaviour
    ///
    /// Each invocation:
    /// 1. Emits a `DeprecWarn` event so off-chain monitors can detect legacy usage.
    /// 2. Increments the `DeprecatedCallCount` storage counter for this entrypoint
    ///    (key index `0`) so operators can measure migration progress.
    /// 3. **Panics** with `"DEPRECATED: use redeem_voucher_from(redeemer, token_id)"`.
    ///    The panic preserves the original fail-fast contract behaviour while the
    ///    on-chain event and counter give callers actionable migration information.
    ///
    /// Callers that still invoke `redeem_voucher` must migrate before the next
    /// major deployment.  Use `get_deprecated_call_count(0)` to monitor how
    /// many legacy calls are still occurring.
    ///
    /// # Panics
    /// Always panics with the migration hint message.
    pub fn redeem_voucher(e: Env, _token_id: u128) {
        // 1. Emit a deprecation-warning event so indexers / monitoring dashboards
        //    can alert operators about remaining legacy callers.
        #[allow(deprecated)]
        e.events().publish(
            (symbol_short!("DeprecWarn"), symbol_short!("rdm_v")),
            symbol_short!("rdm_vf"),
        );

        // 2. Increment the per-entrypoint deprecated-call counter (index 0 =
        //    redeem_voucher).  This persists across calls, giving operators a
        //    cumulative measure of legacy usage.
        Self::_increment_deprecated_call_count(&e, 0);

        // 3. Panic with a clear migration message.  Using panic (rather than
        //    silently delegating) preserves security: the caller must explicitly
        //    provide and authorise their address in redeem_voucher_from.
        panic!("DEPRECATED: use redeem_voucher_from(redeemer, token_id)");
    }

    /// Redeem a voucher, burning the token and transferring the locked TYC to
    /// the redeemer.
    ///
    /// This is the **current canonical redemption entrypoint** (replaces the
    /// deprecated `redeem_voucher`).
    ///
    /// # Arguments
    /// * `redeemer`  — Address that owns the voucher and receives the TYC.
    /// * `token_id`  — Voucher token ID to redeem.
    ///
    /// # Panics
    /// * `"Contract is paused"` — while emergency pause is active.
    /// * `"Invalid token_id"` — voucher does not exist or was already redeemed.
    /// * `"Insufficient balance"` — redeemer does not hold this voucher.
    pub fn redeem_voucher_from(e: Env, redeemer: Address, token_id: u128) {
        redeemer.require_auth();

        let paused: bool = e
            .storage()
            .persistent()
            .get(&DataKey::Paused)
            .unwrap_or(false);
        if paused {
            panic!("Contract is paused");
        }

        // Validate the voucher exists before burning.
        let tyc_value: u128 = e
            .storage()
            .persistent()
            .get(&DataKey::VoucherValue(token_id))
            .expect("Invalid token_id");

        // Burn the single voucher token.
        Self::_burn(&e, redeemer.clone(), token_id, 1);

        // Transfer TYC from contract to redeemer.
        let tyc_token: Address = e
            .storage()
            .persistent()
            .get(&DataKey::TycToken)
            .expect("Not initialized");
        let client = soroban_sdk::token::Client::new(&e, &tyc_token);
        client.transfer(
            &e.current_contract_address(),
            &redeemer,
            &(tyc_value as i128),
        );

        // Clean up voucher value storage.
        e.storage()
            .persistent()
            .remove(&DataKey::VoucherValue(token_id));

        #[allow(deprecated)]
        e.events()
            .publish((symbol_short!("Redeem"), redeemer, token_id), tyc_value);
    }

    // -----------------------------------------------------------------------
    // Token transfer
    // -----------------------------------------------------------------------

    /// Transfer voucher tokens between addresses.
    ///
    /// # Arguments
    /// * `from`     — Current owner (must sign).
    /// * `to`       — Recipient.
    /// * `token_id` — Token to transfer.
    /// * `amount`   — Number of tokens to transfer.
    ///
    /// # Panics
    /// * `"Contract is paused"` — while pause is active.
    /// * `"Transfer amount must be greater than zero"` — zero-amount guard.
    /// * `"Insufficient balance"` — `from` does not hold enough.
    pub fn transfer(e: Env, from: Address, to: Address, token_id: u128, amount: u64) {
        from.require_auth();

        if amount == 0 {
            panic!("Transfer amount must be greater than zero");
        }

        let paused: bool = e
            .storage()
            .persistent()
            .get(&DataKey::Paused)
            .unwrap_or(false);
        if paused {
            panic!("Contract is paused");
        }

        Self::_burn(&e, from.clone(), token_id, amount);
        Self::_mint(&e, to.clone(), token_id, amount);

        #[allow(deprecated)]
        e.events()
            .publish((symbol_short!("Transfer"), from, to, token_id), amount);
    }

    // -----------------------------------------------------------------------
    // Admin withdrawal
    // -----------------------------------------------------------------------

    /// Withdraw tokens held by this contract to a recipient address.
    ///
    /// Only TYC and USDC (configured at initialisation) are allowed.
    ///
    /// # Arguments
    /// * `token`  — Token contract address (must be TYC or USDC).
    /// * `to`     — Recipient of the withdrawn tokens.
    /// * `amount` — Number of tokens to withdraw (> 0).
    ///
    /// # Panics
    /// * `"Not initialized"` — contract not set up.
    /// * `"Withdrawal amount must be greater than zero"` — zero guard.
    /// * `"Invalid token: not in allowlist"` — token not TYC or USDC.
    /// * `"Insufficient contract balance"` — contract holds less than `amount`.
    /// * Authorization failure — caller is not the admin.
    pub fn withdraw_funds(e: Env, token: Address, to: Address, amount: u128) {
        let admin: Address = e
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();

        if amount == 0 {
            panic!("Withdrawal amount must be greater than zero");
        }

        let tyc_token: Address = e
            .storage()
            .persistent()
            .get(&DataKey::TycToken)
            .expect("Not initialized");
        let usdc_token: Address = e
            .storage()
            .persistent()
            .get(&DataKey::UsdcToken)
            .expect("Not initialized");

        if token != tyc_token && token != usdc_token {
            panic!("Invalid token: not in allowlist");
        }

        let token_client = soroban_sdk::token::Client::new(&e, &token);
        let contract_address = e.current_contract_address();
        let balance = token_client.balance(&contract_address);

        if balance < amount as i128 {
            panic!("Insufficient contract balance");
        }

        token_client.transfer(&contract_address, &to, &(amount as i128));

        #[allow(deprecated)]
        e.events()
            .publish((symbol_short!("Withdraw"), token.clone(), to), amount);
    }

    // -----------------------------------------------------------------------
    // Read-only queries
    // -----------------------------------------------------------------------

    /// Return the token balance for `owner` at `token_id`.
    pub fn get_balance(e: Env, owner: Address, token_id: u128) -> u64 {
        Self::balance_of(&e, owner, token_id)
    }

    /// Return the number of distinct voucher token types held by `owner`.
    pub fn owned_token_count(e: Env, owner: Address) -> u32 {
        e.storage()
            .persistent()
            .get(&DataKey::OwnedTokenCount(owner))
            .unwrap_or(0)
    }

    /// Return whether the contract is currently paused.
    pub fn is_paused(e: Env) -> bool {
        e.storage()
            .persistent()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    /// Return the TYC value locked in a voucher, or `None` if not found /
    /// already redeemed.
    pub fn get_voucher_value(e: Env, token_id: u128) -> Option<u128> {
        e.storage()
            .persistent()
            .get(&DataKey::VoucherValue(token_id))
    }

    /// Return the current contract version number.
    ///
    /// Off-chain clients and migrating callers should check this value to
    /// determine which entrypoints are available.  Version history:
    ///
    /// | Version | Notable changes |
    /// |---------|-----------------|
    /// | 1       | Initial deploy: `redeem_voucher`, `mint_voucher`, `transfer` |
    /// | 2       | `redeem_voucher` deprecated; `redeem_voucher_from` introduced; `withdraw_funds`, pause/unpause, backend-minter added |
    pub fn get_contract_version(_e: Env) -> u32 {
        CONTRACT_VERSION
    }

    /// Return how many times deprecated entrypoint `index` has been called.
    ///
    /// | Index | Entrypoint |
    /// |-------|------------|
    /// | 0     | `redeem_voucher` |
    ///
    /// Returns `0` if the entrypoint has never been called or if `index` is
    /// unknown — this is always a safe default and does not panic.
    pub fn get_deprecated_call_count(e: Env, index: u32) -> u32 {
        e.storage()
            .persistent()
            .get(&DataKey::DeprecatedCallCount(index))
            .unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

impl TycoonRewardSystem {
    /// Mint `amount` of `token_id` to `to`, updating the owned-count index.
    fn _mint(e: &Env, to: Address, token_id: u128, amount: u64) {
        if amount == 0 {
            return;
        }
        let key = DataKey::Balance(to.clone(), token_id);
        let current_balance: u64 = e.storage().persistent().get(&key).unwrap_or(0);

        let new_balance = current_balance
            .checked_add(amount)
            .expect("Balance overflow");

        e.storage().persistent().set(&key, &new_balance);

        // Increment owned-count only when the owner receives this token for the
        // first time (i.e., their balance was previously zero).
        if current_balance == 0 {
            let count_key = DataKey::OwnedTokenCount(to.clone());
            let current_count: u32 = e.storage().persistent().get(&count_key).unwrap_or(0);
            e.storage()
                .persistent()
                .set(&count_key, &(current_count + 1));
        }

        #[allow(deprecated)]
        e.events()
            .publish((symbol_short!("Mint"), to, token_id), amount);
    }

    /// Burn `amount` of `token_id` from `from`, updating the owned-count index.
    fn _burn(e: &Env, from: Address, token_id: u128, amount: u64) {
        if amount == 0 {
            return;
        }
        let key = DataKey::Balance(from.clone(), token_id);
        let current_balance: u64 = e.storage().persistent().get(&key).unwrap_or(0);

        if current_balance < amount {
            panic!("Insufficient balance");
        }

        let new_balance = current_balance - amount;

        if new_balance == 0 {
            e.storage().persistent().remove(&key);
        } else {
            e.storage().persistent().set(&key, &new_balance);
        }

        // Decrement owned-count when the owner's balance for this token hits zero.
        if current_balance > 0 && new_balance == 0 {
            let count_key = DataKey::OwnedTokenCount(from.clone());
            let current_count: u32 = e.storage().persistent().get(&count_key).unwrap_or(0);
            if current_count > 0 {
                let updated = current_count - 1;
                if updated == 0 {
                    e.storage().persistent().remove(&count_key);
                } else {
                    e.storage().persistent().set(&count_key, &updated);
                }
            }
        }

        #[allow(deprecated)]
        e.events()
            .publish((symbol_short!("Burn"), from, token_id), amount);
    }

    /// Return the raw token balance for `owner` at `token_id`.
    fn balance_of(e: &Env, owner: Address, token_id: u128) -> u64 {
        let key = DataKey::Balance(owner, token_id);
        e.storage().persistent().get(&key).unwrap_or(0)
    }

    /// Increment the deprecated-call counter for entrypoint `index`.
    ///
    /// Uses saturating addition so an extreme call count can never cause a
    /// storage overflow panic (u32::MAX ≈ 4 billion calls).
    fn _increment_deprecated_call_count(e: &Env, index: u32) {
        let key = DataKey::DeprecatedCallCount(index);
        let current: u32 = e.storage().persistent().get(&key).unwrap_or(0);
        e.storage()
            .persistent()
            .set(&key, &current.saturating_add(1));
    }
}

// ---------------------------------------------------------------------------
// Test-only public wrappers
// ---------------------------------------------------------------------------

#[contractimpl]
impl TycoonRewardSystem {
    /// Expose `_mint` for unit testing without going through business logic.
    pub fn test_mint(e: Env, to: Address, token_id: u128, amount: u64) {
        Self::_mint(&e, to, token_id, amount);
    }

    /// Expose `_burn` for unit testing without going through business logic.
    pub fn test_burn(e: Env, from: Address, token_id: u128, amount: u64) {
        Self::_burn(&e, from, token_id, amount);
    }
}

#[cfg(test)]
mod test;
