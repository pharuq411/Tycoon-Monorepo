extern crate std;

use crate::{DataKey, TycoonRewardSystem, TycoonRewardSystemClient, VOUCHER_ID_START};
use soroban_sdk::testutils::Address as TestAddress;
use soroban_sdk::{token, Address, Env};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Spin up a fully-initialised reward-system contract and return the client,
/// TYC token address, USDC token address, and admin address.
fn setup(
    env: &Env,
) -> (
    TycoonRewardSystemClient,
    Address, // contract_id
    Address, // admin
    Address, // tyc_token_id
    Address, // usdc_token_id
) {
    let admin = Address::generate(env);
    let tyc_admin = Address::generate(env);
    let usdc_admin = Address::generate(env);

    let tyc_token_id = env
        .register_stellar_asset_contract_v2(tyc_admin.clone())
        .address();
    let usdc_token_id = env
        .register_stellar_asset_contract_v2(usdc_admin.clone())
        .address();

    let contract_id = env.register(TycoonRewardSystem, ());
    let client = TycoonRewardSystemClient::new(env, &contract_id);
    client.initialize(&admin, &tyc_token_id, &usdc_token_id);

    (client, contract_id, admin, tyc_token_id, usdc_token_id)
}

/// Fund the reward-system contract with `amount` TYC.
fn fund_tyc(env: &Env, tyc_token_id: &Address, contract_id: &Address, amount: i128) {
    token::StellarAssetClient::new(env, tyc_token_id).mint(contract_id, &amount);
}

// ===========================================================================
// Initialisation
// ===========================================================================

#[test]
fn test_initialize_success() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, contract_id, admin, tyc_token_id, usdc_token_id) = setup(&env);

    // Sanity: contract is not paused after init.
    assert!(!client.is_paused());

    // Admin is stored correctly.
    let stored_admin: Address = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not stored")
    });
    assert_eq!(stored_admin, admin);

    // Token addresses are stored.
    let stored_tyc: Address = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::TycToken)
            .expect("tyc not stored")
    });
    assert_eq!(stored_tyc, tyc_token_id);

    let stored_usdc: Address = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::UsdcToken)
            .expect("usdc not stored")
    });
    assert_eq!(stored_usdc, usdc_token_id);
}

#[test]
fn test_initialize_double_init_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _contract_id, admin, tyc_token_id, usdc_token_id) = setup(&env);

    // Second call must panic with "Already initialized".
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.initialize(&admin, &tyc_token_id, &usdc_token_id);
    }));
    assert!(res.is_err(), "double-init should panic");
}

// ===========================================================================
// Internal _mint / _burn primitives
// ===========================================================================

#[test]
fn test_simple_event() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(TycoonRewardSystem, ());
    let client = TycoonRewardSystemClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    client.test_mint(&user, &123, &10);
    let events = env.events().all();
    std::println!("Simple test events: {}", events.len());
    assert!(!events.is_empty());
}

#[test]
fn test_mint_and_burn_balance_tracking() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(TycoonRewardSystem, ());
    let client = TycoonRewardSystemClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.test_mint(&user, &1, &5);
    assert_eq!(client.get_balance(&user, &1), 5);

    client.test_burn(&user, &1, &3);
    assert_eq!(client.get_balance(&user, &1), 2);

    client.test_burn(&user, &1, &2);
    assert_eq!(client.get_balance(&user, &1), 0);
}

#[test]
fn test_burn_insufficient_balance_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(TycoonRewardSystem, ());
    let client = TycoonRewardSystemClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.test_mint(&user, &1, &2);

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.test_burn(&user, &1, &5);
    }));
    assert!(res.is_err(), "over-burn should panic");
}

#[test]
fn test_burn_from_zero_balance_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(TycoonRewardSystem, ());
    let client = TycoonRewardSystemClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.test_burn(&user, &999, &1);
    }));
    assert!(res.is_err(), "burn with zero balance should panic");
}

// ===========================================================================
// Voucher mint
// ===========================================================================

#[test]
fn test_mint_voucher_assigns_sequential_ids() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, contract_id, admin, tyc_token_id, _) = setup(&env);
    fund_tyc(&env, &tyc_token_id, &contract_id, 10_000);

    let user = Address::generate(&env);
    let id1 = client.mint_voucher(&admin, &user, &100);
    let id2 = client.mint_voucher(&admin, &user, &200);

    assert_eq!(id2, id1 + 1);
    assert!(id1 >= VOUCHER_ID_START);
}

#[test]
fn test_mint_voucher_zero_value_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, admin, _, _) = setup(&env);
    let user = Address::generate(&env);

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.mint_voucher(&admin, &user, &0);
    }));
    assert!(res.is_err(), "zero-value voucher should panic");
}

#[test]
fn test_mint_voucher_unauthorized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _) = setup(&env);
    let attacker = Address::generate(&env);
    let user = Address::generate(&env);

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.mint_voucher(&attacker, &user, &500);
    }));
    assert!(res.is_err(), "non-admin should not be able to mint");
}

// ===========================================================================
// Full voucher flow
// ===========================================================================

#[test]
fn test_voucher_flow() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, contract_id, admin, tyc_token_id, _) = setup(&env);
    let user = Address::generate(&env);

    fund_tyc(&env, &tyc_token_id, &contract_id, 10_000);
    let tyc_token = token::Client::new(&env, &tyc_token_id);

    // Mint.
    let tyc_value: u128 = 500;
    let token_id = client.mint_voucher(&admin, &user, &tyc_value);
    assert_eq!(client.get_balance(&user, &token_id), 1);
    assert_eq!(client.get_voucher_value(&token_id), Some(500));

    // Redeem.
    client.redeem_voucher_from(&user, &token_id);

    assert_eq!(tyc_token.balance(&user), 500);
    assert_eq!(tyc_token.balance(&contract_id), 9_500);
    assert_eq!(client.get_balance(&user, &token_id), 0);
    // Voucher storage cleaned up.
    assert_eq!(client.get_voucher_value(&token_id), None);
}

#[test]
fn test_redeem_already_redeemed_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, contract_id, admin, tyc_token_id, _) = setup(&env);
    let user = Address::generate(&env);

    fund_tyc(&env, &tyc_token_id, &contract_id, 10_000);

    let token_id = client.mint_voucher(&admin, &user, &500);
    client.redeem_voucher_from(&user, &token_id);

    // Second redeem must fail (balance = 0, storage gone).
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.redeem_voucher_from(&user, &token_id);
    }));
    assert!(res.is_err(), "double-redeem should panic");
}

#[test]
fn test_redeem_nonexistent_token_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _) = setup(&env);
    let user = Address::generate(&env);

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.redeem_voucher_from(&user, &9_999_999_999);
    }));
    assert!(res.is_err(), "redeeming non-existent token should panic");
}

#[test]
fn test_redeem_wrong_owner_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, contract_id, admin, tyc_token_id, _) = setup(&env);
    let owner = Address::generate(&env);
    let attacker = Address::generate(&env);

    fund_tyc(&env, &tyc_token_id, &contract_id, 10_000);
    let token_id = client.mint_voucher(&admin, &owner, &500);

    // Attacker tries to redeem a voucher they don't own.
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.redeem_voucher_from(&attacker, &token_id);
    }));
    assert!(res.is_err(), "wrong owner redemption should panic");
}

// ===========================================================================
// Pause / Unpause
// ===========================================================================

#[test]
fn test_pause_and_unpause() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, contract_id, _, _, _) = setup(&env);

    assert!(!client.is_paused());

    client.pause();
    assert!(client.is_paused());

    client.unpause();
    assert!(!client.is_paused());

    let _ = contract_id; // suppress unused warning
}

#[test]
fn test_redeem_fails_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, contract_id, admin, tyc_token_id, _) = setup(&env);
    let user = Address::generate(&env);

    fund_tyc(&env, &tyc_token_id, &contract_id, 10_000);
    let token_id = client.mint_voucher(&admin, &user, &500);

    client.pause();

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.redeem_voucher_from(&user, &token_id);
    }));
    assert!(res.is_err(), "redeem while paused should fail");

    client.unpause();

    // Succeeds after unpause.
    client.redeem_voucher_from(&user, &token_id);
    assert_eq!(
        token::Client::new(&env, &tyc_token_id).balance(&user),
        500
    );
}

#[test]
fn test_transfer_fails_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, contract_id, admin, tyc_token_id, _) = setup(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    fund_tyc(&env, &tyc_token_id, &contract_id, 10_000);
    let token_id = client.mint_voucher(&admin, &user1, &500);

    client.pause();

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.transfer(&user1, &user2, &token_id, &1);
    }));
    assert!(res.is_err(), "transfer while paused should fail");
}

// ===========================================================================
// Backend minter
// ===========================================================================

#[test]
fn test_set_backend_minter_admin_only() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, admin, _, _) = setup(&env);
    let minter = Address::generate(&env);

    client.set_backend_minter(&admin, &minter);
    assert_eq!(client.get_backend_minter(), Some(minter));
}

#[test]
fn test_set_backend_minter_unauthorized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _) = setup(&env);
    let attacker = Address::generate(&env);
    let minter = Address::generate(&env);

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.set_backend_minter(&attacker, &minter);
    }));
    assert!(res.is_err(), "non-admin cannot set backend minter");
}

#[test]
fn test_backend_minter_can_mint() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, contract_id, admin, tyc_token_id, _) = setup(&env);
    let minter = Address::generate(&env);
    let user = Address::generate(&env);

    fund_tyc(&env, &tyc_token_id, &contract_id, 10_000);
    client.set_backend_minter(&admin, &minter);

    let token_id = client.mint_voucher(&minter, &user, &300);
    assert_eq!(client.get_balance(&user, &token_id), 1);
}

#[test]
fn test_non_admin_non_minter_cannot_mint() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, contract_id, admin, tyc_token_id, _) = setup(&env);
    let minter = Address::generate(&env);
    let attacker = Address::generate(&env);
    let user = Address::generate(&env);

    fund_tyc(&env, &tyc_token_id, &contract_id, 10_000);
    client.set_backend_minter(&admin, &minter);

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.mint_voucher(&attacker, &user, &300);
    }));
    assert!(res.is_err(), "unlisted caller should not mint");
}

#[test]
fn test_clear_backend_minter() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, contract_id, admin, tyc_token_id, _) = setup(&env);
    let minter = Address::generate(&env);
    let user = Address::generate(&env);

    fund_tyc(&env, &tyc_token_id, &contract_id, 10_000);
    client.set_backend_minter(&admin, &minter);
    assert!(client.get_backend_minter().is_some());

    client.clear_backend_minter(&admin);

    // After clearing, the former minter cannot mint.
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.mint_voucher(&minter, &user, &100);
    }));
    assert!(res.is_err(), "cleared minter should not be able to mint");
}

// ===========================================================================
// Transfer & owned_token_count
// ===========================================================================

#[test]
fn test_transfer_updates_balances_and_counts() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, contract_id, admin, tyc_token_id, _) = setup(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    fund_tyc(&env, &tyc_token_id, &contract_id, 10_000);

    let id1 = client.mint_voucher(&admin, &user1, &100);
    let id2 = client.mint_voucher(&admin, &user1, &200);

    assert_eq!(client.owned_token_count(&user1), 2);
    assert_eq!(client.owned_token_count(&user2), 0);

    // Transfer id1 from user1 to user2.
    client.transfer(&user1, &user2, &id1, &1);

    assert_eq!(client.owned_token_count(&user1), 1);
    assert_eq!(client.owned_token_count(&user2), 1);

    // Transfer id2 as well.
    client.transfer(&user1, &user2, &id2, &1);

    assert_eq!(client.owned_token_count(&user1), 0);
    assert_eq!(client.owned_token_count(&user2), 2);

    // user2 redeems both.
    client.redeem_voucher_from(&user2, &id1);
    assert_eq!(client.owned_token_count(&user2), 1);

    client.redeem_voucher_from(&user2, &id2);
    assert_eq!(client.owned_token_count(&user2), 0);
}

#[test]
fn test_transfer_zero_amount_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, contract_id, admin, tyc_token_id, _) = setup(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    fund_tyc(&env, &tyc_token_id, &contract_id, 10_000);
    let token_id = client.mint_voucher(&admin, &user1, &100);

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.transfer(&user1, &user2, &token_id, &0);
    }));
    assert!(res.is_err(), "zero-amount transfer should panic");
}

#[test]
fn test_owned_token_count_empty_address() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _) = setup(&env);
    let stranger = Address::generate(&env);
    assert_eq!(client.owned_token_count(&stranger), 0);
}

// ===========================================================================
// Withdraw funds
// ===========================================================================

#[test]
fn test_withdraw_funds_admin_can_withdraw() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, contract_id, _, tyc_token_id, usdc_token_id) = setup(&env);
    let recipient = Address::generate(&env);

    let tyc = token::Client::new(&env, &tyc_token_id);
    let usdc = token::Client::new(&env, &usdc_token_id);

    token::StellarAssetClient::new(&env, &tyc_token_id).mint(&contract_id, &5_000);
    token::StellarAssetClient::new(&env, &usdc_token_id).mint(&contract_id, &1_000);

    client.withdraw_funds(&tyc_token_id, &recipient, &2_000);
    assert_eq!(tyc.balance(&contract_id), 3_000);
    assert_eq!(tyc.balance(&recipient), 2_000);

    client.withdraw_funds(&usdc_token_id, &recipient, &500);
    assert_eq!(usdc.balance(&contract_id), 500);
    assert_eq!(usdc.balance(&recipient), 500);
}

#[test]
fn test_withdraw_funds_non_admin_reverts() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, contract_id, _, tyc_token_id, _) = setup(&env);
    let recipient = Address::generate(&env);

    token::StellarAssetClient::new(&env, &tyc_token_id).mint(&contract_id, &5_000);

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        env.as_contract(&contract_id, || {
            let non_admin_client = TycoonRewardSystemClient::new(&env, &contract_id);
            non_admin_client.withdraw_funds(&tyc_token_id, &recipient, &1_000);
        });
    }));
    assert!(res.is_err(), "non-admin withdraw should fail");
}

#[test]
fn test_withdraw_funds_zero_amount_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, contract_id, _, tyc_token_id, _) = setup(&env);
    let recipient = Address::generate(&env);

    token::StellarAssetClient::new(&env, &tyc_token_id).mint(&contract_id, &5_000);

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.withdraw_funds(&tyc_token_id, &recipient, &0);
    }));
    assert!(res.is_err(), "zero-amount withdrawal should panic");
}

#[test]
fn test_withdraw_funds_insufficient_balance_reverts() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, contract_id, _, tyc_token_id, _) = setup(&env);
    let recipient = Address::generate(&env);

    token::StellarAssetClient::new(&env, &tyc_token_id).mint(&contract_id, &1_000);

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.withdraw_funds(&tyc_token_id, &recipient, &5_000);
    }));
    assert!(res.is_err(), "over-withdrawal should fail");
}

#[test]
fn test_withdraw_funds_invalid_token_reverts() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, contract_id, _, tyc_token_id, _) = setup(&env);
    let recipient = Address::generate(&env);

    let rogue_admin = Address::generate(&env);
    let rogue_token = env
        .register_stellar_asset_contract_v2(rogue_admin)
        .address();

    token::StellarAssetClient::new(&env, &tyc_token_id).mint(&contract_id, &5_000);

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.withdraw_funds(&rogue_token, &recipient, &1_000);
    }));
    assert!(res.is_err(), "unlisted token withdrawal should fail");
}

// ===========================================================================
// Stale / disconnected / invalid state guards
// ===========================================================================

#[test]
fn test_get_voucher_value_returns_none_for_unknown_id() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _) = setup(&env);
    // An ID that was never minted should return None, not panic.
    assert_eq!(client.get_voucher_value(&42_000_000_000), None);
}

#[test]
fn test_get_balance_unknown_owner_returns_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _) = setup(&env);
    let stranger = Address::generate(&env);
    // Should return 0, not panic.
    assert_eq!(client.get_balance(&stranger, &1), 0);
}

#[test]
fn test_is_paused_before_init_returns_false() {
    let env = Env::default();
    env.mock_all_auths();
    // Fresh contract — no init call.
    let contract_id = env.register(TycoonRewardSystem, ());
    let client = TycoonRewardSystemClient::new(&env, &contract_id);
    assert!(!client.is_paused());
}
