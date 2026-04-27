//! Generic conformance tests for the [`Storage`] trait.
//!
//! Each public `async fn` tests one contract of the storage layer.
//! The [`storage_tests!`] macro wires them into concrete `#[tokio::test]`
//! functions for a given implementation.
//!
//! Enable with the `test-support` feature:
//!
//! ```toml
//! [dev-dependencies]
//! ledger-core = { path = "../ledger-core", features = ["test-support"] }
//! ```
//!
//! Then invoke:
//!
//! ```ignore
//! #[cfg(test)]
//! mod my_storage_tests {
//!     use ledger_core::storage::test_support::storage_tests;
//!     storage_tests!(async { MyStorage::connect(":memory:").await });
//! }
//! ```

use crate::{
    Asset, EntryRef, LedgerError, SpendingToken, Storage, TokenStatus, Transaction,
    TransactionBuilder,
};

// ── Helpers ──────────────────────────────────────────────────────────

fn brush() -> Asset {
    Asset::new("brush", 0)
}

fn usd() -> Asset {
    Asset::new("usd", 2)
}

/// Register brush and usd assets so SQLite JOINs succeed.
async fn register_test_assets(s: &dyn Storage) {
    s.register_asset(&brush()).await.expect("register brush");
    s.register_asset(&usd()).await.expect("register usd");
}

/// Build a balanced issuance transaction (credit account + debit @world)
/// and the spending tokens the storage layer should persist.
fn make_issuance(
    key: &str,
    account: &str,
    asset: &Asset,
    raw: i128,
) -> (Transaction, Vec<SpendingToken>) {
    let amount = asset.try_amount(raw);
    let neg = amount.negate();
    let tx = TransactionBuilder::new(key)
        .credit(account, &amount)
        .credit("@world", &neg)
        .build()
        .expect("build issuance fixture");

    let tokens = vec![SpendingToken {
        entry_ref: EntryRef {
            tx_id: tx.tx_id.clone(),
            entry_index: 0,
        },
        owner: account.to_string(),
        amount,
        status: TokenStatus::Unspent,
    }];

    (tx, tokens)
}

/// Build a transfer that spends one token and creates a new one.
fn make_transfer(
    key: &str,
    spent_ref: &EntryRef,
    from: &str,
    to: &str,
    asset: &Asset,
    raw: i128,
) -> (Transaction, Vec<SpendingToken>, Vec<EntryRef>) {
    let amount = asset.try_amount(raw);
    let tx = TransactionBuilder::new(key)
        .debit(&spent_ref.tx_id, spent_ref.entry_index, from, &amount)
        .credit(to, &amount)
        .build()
        .expect("build transfer fixture");

    let new_tokens = vec![SpendingToken {
        entry_ref: EntryRef {
            tx_id: tx.tx_id.clone(),
            entry_index: 0,
        },
        owner: to.to_string(),
        amount,
        status: TokenStatus::Unspent,
    }];

    let spent = vec![spent_ref.clone()];
    (tx, new_tokens, spent)
}

// ── Asset tests ──────────────────────────────────────────────────────

pub async fn load_assets_empty(s: &dyn Storage) {
    let assets = s.load_assets().await.expect("load_assets on empty storage");
    assert!(assets.is_empty());
}

pub async fn save_and_load_asset(s: &dyn Storage) {
    let brush = Asset::new("brush", 0);
    s.register_asset(&brush).await.expect("save brush");

    let loaded = s.load_assets().await.expect("load_assets");
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded["brush"], brush);
}

pub async fn register_asset_duplicate_is_noop(s: &dyn Storage) {
    let brush = Asset::new("brush", 0);
    s.register_asset(&brush).await.expect("save brush");
    s.register_asset(&brush)
        .await
        .expect("duplicate save should be a no-op");

    let loaded = s.load_assets().await.expect("load_assets");
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded["brush"], brush);
}

pub async fn register_asset_conflict_rejected(s: &dyn Storage) {
    let v1 = Asset::new("thing", 0);
    s.register_asset(&v1).await.expect("save v1");

    let v2 = Asset::new("thing", 2);
    let err = s
        .register_asset(&v2)
        .await
        .expect_err("conflicting save should fail");
    assert!(
        matches!(err, LedgerError::AssetConflict { .. }),
        "expected AssetConflict, got {err:?}"
    );

    // Original asset unchanged.
    let loaded = s.load_assets().await.expect("load_assets");
    assert_eq!(loaded["thing"], v1);
}

pub async fn save_multiple_assets(s: &dyn Storage) {
    let brush = Asset::new("brush", 0);
    let usd = Asset::new("usd", 2);
    s.register_asset(&brush).await.expect("save brush");
    s.register_asset(&usd).await.expect("save usd");

    let loaded = s.load_assets().await.expect("load_assets");
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded["brush"], brush);
    assert_eq!(loaded["usd"], usd);
}

// ── Idempotency key tests ────────────────────────────────────────────

pub async fn has_key_empty(s: &dyn Storage) {
    assert!(!s.has_idempotency_key("anything").await.expect("has_key"));
}

pub async fn key_recorded_after_commit(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx, tokens) = make_issuance("issue-001", "a", &brush(), 5);
    s.commit_tx(&tx, &tokens, &[]).await.expect("commit");
    assert!(s.has_idempotency_key("issue-001").await.expect("has_key"));
}

pub async fn key_absent_for_uncommitted(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx, tokens) = make_issuance("issue-001", "a", &brush(), 5);
    s.commit_tx(&tx, &tokens, &[]).await.expect("commit");
    assert!(!s.has_idempotency_key("other-key").await.expect("has_key"));
}

// ── Token tests ──────────────────────────────────────────────────────

pub async fn get_token_nonexistent(s: &dyn Storage) {
    let eref = EntryRef {
        tx_id: "nonexistent".to_string(),
        entry_index: 0,
    };
    assert!(s.get_token(&eref).await.expect("get_token").is_none());
}

pub async fn get_token_after_commit(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx, tokens) = make_issuance("issue-001", "a", &brush(), 5);
    let eref = tokens[0].entry_ref.clone();
    s.commit_tx(&tx, &tokens, &[]).await.expect("commit");

    let token = s
        .get_token(&eref)
        .await
        .expect("get_token")
        .expect("token should exist");
    assert_eq!(token.amount.raw(), 5);
    assert_eq!(token.amount.asset_name(), "brush");
    assert_eq!(token.status, TokenStatus::Unspent);
}

pub async fn token_marked_spent(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx1, tokens1) = make_issuance("issue-001", "a", &brush(), 5);
    let eref = tokens1[0].entry_ref.clone();
    s.commit_tx(&tx1, &tokens1, &[])
        .await
        .expect("commit issuance");

    let (tx2, tokens2, spent) = make_transfer("xfer-001", &eref, "a", "b", &brush(), 5);
    s.commit_tx(&tx2, &tokens2, &spent)
        .await
        .expect("commit transfer");

    let token = s
        .get_token(&eref)
        .await
        .expect("get_token")
        .expect("token should exist");
    assert!(matches!(token.status, TokenStatus::Spent(_)));
}

// ── Unspent by account tests ─────────────────────────────────────────

pub async fn unspent_account_empty(s: &dyn Storage) {
    register_test_assets(s).await;
    let result = s
        .unspent_by_account("nobody", Some(&brush().max()))
        .await
        .expect("unspent_by_account");
    assert!(result.is_empty());
}

pub async fn unspent_account_returns_matching(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx, tokens) = make_issuance("issue-001", "a", &brush(), 5);
    s.commit_tx(&tx, &tokens, &[]).await.expect("commit");

    let result = s
        .unspent_by_account("a", Some(&brush().max()))
        .await
        .expect("unspent_by_account");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].amount.raw(), 5);
}

pub async fn unspent_account_excludes_spent(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx1, tokens1) = make_issuance("issue-001", "a", &brush(), 5);
    let eref = tokens1[0].entry_ref.clone();
    s.commit_tx(&tx1, &tokens1, &[])
        .await
        .expect("commit issuance");

    let (tx2, tokens2, spent) = make_transfer("xfer-001", &eref, "a", "b", &brush(), 5);
    s.commit_tx(&tx2, &tokens2, &spent)
        .await
        .expect("commit transfer");

    let result = s
        .unspent_by_account("a", Some(&brush().max()))
        .await
        .expect("unspent_by_account");
    assert!(result.is_empty(), "spent tokens should be excluded");
}

pub async fn unspent_account_excludes_other_assets(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx, tokens) = make_issuance("issue-001", "a", &brush(), 5);
    s.commit_tx(&tx, &tokens, &[]).await.expect("commit");

    let result = s
        .unspent_by_account("a", Some(&usd().max()))
        .await
        .expect("unspent_by_account");
    assert!(result.is_empty(), "different asset should be excluded");
}

pub async fn unspent_account_excludes_children(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx, tokens) = make_issuance("issue-001", "store1/inventory", &brush(), 5);
    s.commit_tx(&tx, &tokens, &[]).await.expect("commit");

    let result = s
        .unspent_by_account("store1", Some(&brush().max()))
        .await
        .expect("unspent_by_account");
    assert!(
        result.is_empty(),
        "child account tokens should not appear in exact account query"
    );
}

// ── Unspent by prefix tests ─────────────────────────────────────────

pub async fn unspent_prefix_empty(s: &dyn Storage) {
    register_test_assets(s).await;
    let result = s
        .unspent_by_prefix("nobody", Some(&brush().max()))
        .await
        .expect("unspent_by_prefix");
    assert!(result.is_empty());
}

pub async fn unspent_prefix_includes_descendants(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx, tokens) = make_issuance("issue-001", "store1/inventory", &brush(), 5);
    s.commit_tx(&tx, &tokens, &[]).await.expect("commit");

    let result = s
        .unspent_by_prefix("store1", Some(&brush().max()))
        .await
        .expect("unspent_by_prefix");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].amount.raw(), 5);
}

pub async fn unspent_prefix_includes_exact(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx, tokens) = make_issuance("issue-001", "store1", &brush(), 5);
    s.commit_tx(&tx, &tokens, &[]).await.expect("commit");

    let result = s
        .unspent_by_prefix("store1", Some(&brush().max()))
        .await
        .expect("unspent_by_prefix");
    assert_eq!(result.len(), 1);
}

pub async fn unspent_prefix_excludes_spent(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx1, tokens1) = make_issuance("issue-001", "store1/inventory", &brush(), 5);
    let eref = tokens1[0].entry_ref.clone();
    s.commit_tx(&tx1, &tokens1, &[])
        .await
        .expect("commit issuance");

    let (tx2, tokens2, spent) =
        make_transfer("xfer-001", &eref, "store1/inventory", "b", &brush(), 5);
    s.commit_tx(&tx2, &tokens2, &spent)
        .await
        .expect("commit transfer");

    let result = s
        .unspent_by_prefix("store1", Some(&brush().max()))
        .await
        .expect("unspent_by_prefix");
    assert!(
        result.is_empty(),
        "spent tokens should be excluded from prefix query"
    );
}

pub async fn unspent_prefix_excludes_other_assets(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx, tokens) = make_issuance("issue-001", "store1", &brush(), 5);
    s.commit_tx(&tx, &tokens, &[]).await.expect("commit");

    let result = s
        .unspent_by_prefix("store1", Some(&usd().max()))
        .await
        .expect("unspent_by_prefix");
    assert!(result.is_empty(), "different asset should be excluded");
}

pub async fn unspent_prefix_excludes_non_descendants(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx, tokens) = make_issuance("issue-001", "store2", &brush(), 5);
    s.commit_tx(&tx, &tokens, &[]).await.expect("commit");

    let result = s
        .unspent_by_prefix("store1", Some(&brush().max()))
        .await
        .expect("unspent_by_prefix");
    assert!(
        result.is_empty(),
        "non-descendant accounts should be excluded"
    );
}

// ── Transaction tests ────────────────────────────────────────────────

pub async fn load_transactions_empty(s: &dyn Storage) {
    let txs = s.load_transactions().await.expect("load_transactions");
    assert!(txs.is_empty());
}

pub async fn tx_count_empty(s: &dyn Storage) {
    assert_eq!(s.tx_count().await.expect("tx_count"), 0);
}

pub async fn commit_and_load_transaction(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx, tokens) = make_issuance("issue-001", "a", &brush(), 5);
    let expected_id = tx.tx_id.clone();
    s.commit_tx(&tx, &tokens, &[]).await.expect("commit");

    let txs = s.load_transactions().await.expect("load_transactions");
    assert_eq!(txs.len(), 1);
    assert_eq!(txs[0].tx_id, expected_id);
    assert_eq!(s.tx_count().await.expect("tx_count"), 1);
}

pub async fn transactions_preserve_order(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx1, tokens1) = make_issuance("issue-001", "a", &brush(), 5);
    let (tx2, tokens2) = make_issuance("issue-002", "b", &brush(), 3);
    let id1 = tx1.tx_id.clone();
    let id2 = tx2.tx_id.clone();

    s.commit_tx(&tx1, &tokens1, &[]).await.expect("commit tx1");
    s.commit_tx(&tx2, &tokens2, &[]).await.expect("commit tx2");

    let txs = s.load_transactions().await.expect("load_transactions");
    assert_eq!(txs.len(), 2);
    assert_eq!(txs[0].tx_id, id1, "first committed should be first loaded");
    assert_eq!(
        txs[1].tx_id, id2,
        "second committed should be second loaded"
    );
}

// ── Combined / atomicity tests ───────────────────────────────────────

pub async fn commit_creates_tokens_and_key(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx, tokens) = make_issuance("issue-001", "a", &brush(), 5);
    let eref = tokens[0].entry_ref.clone();
    s.commit_tx(&tx, &tokens, &[]).await.expect("commit");

    let token = s
        .get_token(&eref)
        .await
        .expect("get_token")
        .expect("token should exist");
    assert_eq!(token.amount.raw(), 5);

    assert!(s.has_idempotency_key("issue-001").await.expect("has_key"));
    assert_eq!(s.tx_count().await.expect("tx_count"), 1);
}

pub async fn commit_spends_and_creates(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx1, tokens1) = make_issuance("issue-001", "a", &brush(), 5);
    let eref_a = tokens1[0].entry_ref.clone();
    s.commit_tx(&tx1, &tokens1, &[])
        .await
        .expect("commit issuance");

    let (tx2, tokens2, spent) = make_transfer("xfer-001", &eref_a, "a", "b", &brush(), 5);
    let eref_b = tokens2[0].entry_ref.clone();
    s.commit_tx(&tx2, &tokens2, &spent)
        .await
        .expect("commit transfer");

    let a = s
        .get_token(&eref_a)
        .await
        .expect("get_token A")
        .expect("A should exist");
    assert!(matches!(a.status, TokenStatus::Spent(_)));

    let b = s
        .get_token(&eref_b)
        .await
        .expect("get_token B")
        .expect("B should exist");
    assert_eq!(b.status, TokenStatus::Unspent);

    assert!(s
        .unspent_by_account("a", Some(&brush().max()))
        .await
        .expect("unspent @a")
        .is_empty());
    assert_eq!(
        s.unspent_by_account("b", Some(&brush().max()))
            .await
            .expect("unspent @b")
            .len(),
        1
    );

    assert_eq!(s.tx_count().await.expect("tx_count"), 2);
}

// ── Unspent all by prefix tests ────────────────────────────────────

pub async fn unspent_all_prefix_empty(s: &dyn Storage) {
    register_test_assets(s).await;
    let result = s
        .unspent_by_prefix("nobody", None)
        .await
        .expect("unspent_by_prefix");
    assert!(result.is_empty());
}

pub async fn unspent_all_prefix_returns_multiple_assets(s: &dyn Storage) {
    register_test_assets(s).await;

    let (tx1, tokens1) = make_issuance("issue-001", "store1/inventory", &brush(), 5);
    s.commit_tx(&tx1, &tokens1, &[])
        .await
        .expect("commit brush");

    let (tx2, tokens2) = make_issuance("issue-002", "store1/cash", &usd(), 1000);
    s.commit_tx(&tx2, &tokens2, &[]).await.expect("commit usd");

    let result = s
        .unspent_by_prefix("store1", None)
        .await
        .expect("unspent_by_prefix");
    assert_eq!(result.len(), 2);

    let brush_count = result
        .iter()
        .filter(|t| t.amount.asset_name() == "brush")
        .count();
    let usd_count = result
        .iter()
        .filter(|t| t.amount.asset_name() == "usd")
        .count();
    assert_eq!(brush_count, 1);
    assert_eq!(usd_count, 1);
}

pub async fn unspent_all_prefix_excludes_spent(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx1, tokens1) = make_issuance("issue-001", "a", &brush(), 5);
    let eref = tokens1[0].entry_ref.clone();
    s.commit_tx(&tx1, &tokens1, &[]).await.expect("commit");

    let (tx2, tokens2, spent) = make_transfer("xfer-001", &eref, "a", "b", &brush(), 5);
    s.commit_tx(&tx2, &tokens2, &spent).await.expect("commit");

    let result = s
        .unspent_by_prefix("a", None)
        .await
        .expect("unspent_by_prefix");
    assert!(result.is_empty());
}

pub async fn unspent_all_prefix_excludes_non_descendants(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx, tokens) = make_issuance("issue-001", "store2", &brush(), 5);
    s.commit_tx(&tx, &tokens, &[]).await.expect("commit");

    let result = s
        .unspent_by_prefix("store1", None)
        .await
        .expect("unspent_by_prefix");
    assert!(result.is_empty());
}

// ── Balances by prefix tests ──────────────────────────────────────

pub async fn balances_prefix_empty(s: &dyn Storage) {
    register_test_assets(s).await;
    let result = s
        .balances_by_prefix("nobody")
        .await
        .expect("balances_by_prefix");
    assert!(result.is_empty());
}

pub async fn balances_prefix_groups_by_account_and_asset(s: &dyn Storage) {
    register_test_assets(s).await;

    // Two products in two warehouses
    let (tx1, tokens1) = make_issuance("issue-001", "store/w1/product/1", &brush(), 5);
    s.commit_tx(&tx1, &tokens1, &[]).await.expect("commit");

    let (tx2, tokens2) = make_issuance("issue-002", "store/w2/product/1", &brush(), 3);
    s.commit_tx(&tx2, &tokens2, &[]).await.expect("commit");

    let (tx3, tokens3) = make_issuance("issue-003", "store/w1/product/1", &usd(), 1000);
    s.commit_tx(&tx3, &tokens3, &[]).await.expect("commit");

    let result = s
        .balances_by_prefix("store")
        .await
        .expect("balances_by_prefix");

    // Should have 3 entries: (w1/p1, brush), (w2/p1, brush), (w1/p1, usd)
    assert_eq!(result.len(), 3);

    let w1_brush = result
        .iter()
        .find(|e| e.account == "store/w1/product/1" && e.amount.asset_name() == "brush")
        .expect("w1 brush entry");
    assert_eq!(w1_brush.amount.raw(), 5);

    let w2_brush = result
        .iter()
        .find(|e| e.account == "store/w2/product/1" && e.amount.asset_name() == "brush")
        .expect("w2 brush entry");
    assert_eq!(w2_brush.amount.raw(), 3);

    let w1_usd = result
        .iter()
        .find(|e| e.account == "store/w1/product/1" && e.amount.asset_name() == "usd")
        .expect("w1 usd entry");
    assert_eq!(w1_usd.amount.raw(), 1000);
}

pub async fn balances_prefix_sums_multiple_tokens(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx1, tokens1) = make_issuance("issue-001", "a/sub", &brush(), 5);
    s.commit_tx(&tx1, &tokens1, &[]).await.expect("commit");

    let (tx2, tokens2) = make_issuance("issue-002", "a/sub", &brush(), 3);
    s.commit_tx(&tx2, &tokens2, &[]).await.expect("commit");

    let result = s.balances_by_prefix("a").await.expect("balances_by_prefix");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].amount.raw(), 8);
}

pub async fn balances_prefix_excludes_spent(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx1, tokens1) = make_issuance("issue-001", "a", &brush(), 5);
    let eref = tokens1[0].entry_ref.clone();
    s.commit_tx(&tx1, &tokens1, &[]).await.expect("commit");

    let (tx2, tokens2, spent) = make_transfer("xfer-001", &eref, "a", "b", &brush(), 5);
    s.commit_tx(&tx2, &tokens2, &spent).await.expect("commit");

    let result = s.balances_by_prefix("a").await.expect("balances_by_prefix");
    assert!(
        result.is_empty(),
        "spent tokens should not contribute to balance"
    );
}

pub async fn balances_prefix_excludes_non_descendants(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx, tokens) = make_issuance("issue-001", "store2", &brush(), 5);
    s.commit_tx(&tx, &tokens, &[]).await.expect("commit");

    let result = s
        .balances_by_prefix("store1")
        .await
        .expect("balances_by_prefix");
    assert!(result.is_empty());
}

pub async fn balances_prefix_omits_zero_balances(s: &dyn Storage) {
    register_test_assets(s).await;
    // Create and fully spend a token — net balance is 0
    let (tx1, tokens1) = make_issuance("issue-001", "a", &brush(), 5);
    let eref = tokens1[0].entry_ref.clone();
    s.commit_tx(&tx1, &tokens1, &[]).await.expect("commit");

    // Transfer all to @a/sub (still under prefix @a)
    let (tx2, tokens2, spent) = make_transfer("xfer-001", &eref, "a", "a/sub", &brush(), 5);
    s.commit_tx(&tx2, &tokens2, &spent).await.expect("commit");

    let result = s.balances_by_prefix("a").await.expect("balances_by_prefix");

    // @a has 0 balance (spent), @a/sub has 5 — only @a/sub should appear
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].account, "a/sub");
    assert_eq!(result[0].amount.raw(), 5);
}

// ── Test macro ───────────────────────────────────────────────────────

/// Generate a full conformance test suite for a [`Storage`] implementation.
///
/// The constructor is an async block that returns an `impl Storage`.
/// This allows async setup (e.g., database pool initialization).
///
/// ```ignore
/// #[cfg(test)]
/// mod my_tests {
///     use ledger_core::storage::test_support::storage_tests;
///     storage_tests!(async { MyStorage::connect(":memory:").await });
/// }
/// ```
#[macro_export]
macro_rules! storage_tests {
    ($constructor:expr) => {
        // Assets
        #[tokio::test]
        async fn load_assets_empty() {
            let s = $constructor.await;
            $crate::storage::test_support::load_assets_empty(&s).await;
        }
        #[tokio::test]
        async fn save_and_load_asset() {
            let s = $constructor.await;
            $crate::storage::test_support::save_and_load_asset(&s).await;
        }
        #[tokio::test]
        async fn register_asset_duplicate_is_noop() {
            let s = $constructor.await;
            $crate::storage::test_support::register_asset_duplicate_is_noop(&s).await;
        }
        #[tokio::test]
        async fn register_asset_conflict_rejected() {
            let s = $constructor.await;
            $crate::storage::test_support::register_asset_conflict_rejected(&s).await;
        }
        #[tokio::test]
        async fn save_multiple_assets() {
            let s = $constructor.await;
            $crate::storage::test_support::save_multiple_assets(&s).await;
        }

        // Idempotency
        #[tokio::test]
        async fn has_key_empty() {
            let s = $constructor.await;
            $crate::storage::test_support::has_key_empty(&s).await;
        }
        #[tokio::test]
        async fn key_recorded_after_commit() {
            let s = $constructor.await;
            $crate::storage::test_support::key_recorded_after_commit(&s).await;
        }
        #[tokio::test]
        async fn key_absent_for_uncommitted() {
            let s = $constructor.await;
            $crate::storage::test_support::key_absent_for_uncommitted(&s).await;
        }

        // Tokens
        #[tokio::test]
        async fn get_token_nonexistent() {
            let s = $constructor.await;
            $crate::storage::test_support::get_token_nonexistent(&s).await;
        }
        #[tokio::test]
        async fn get_token_after_commit() {
            let s = $constructor.await;
            $crate::storage::test_support::get_token_after_commit(&s).await;
        }
        #[tokio::test]
        async fn token_marked_spent() {
            let s = $constructor.await;
            $crate::storage::test_support::token_marked_spent(&s).await;
        }

        // Unspent by account
        #[tokio::test]
        async fn unspent_account_empty() {
            let s = $constructor.await;
            $crate::storage::test_support::unspent_account_empty(&s).await;
        }
        #[tokio::test]
        async fn unspent_account_returns_matching() {
            let s = $constructor.await;
            $crate::storage::test_support::unspent_account_returns_matching(&s).await;
        }
        #[tokio::test]
        async fn unspent_account_excludes_spent() {
            let s = $constructor.await;
            $crate::storage::test_support::unspent_account_excludes_spent(&s).await;
        }
        #[tokio::test]
        async fn unspent_account_excludes_other_assets() {
            let s = $constructor.await;
            $crate::storage::test_support::unspent_account_excludes_other_assets(&s).await;
        }
        #[tokio::test]
        async fn unspent_account_excludes_children() {
            let s = $constructor.await;
            $crate::storage::test_support::unspent_account_excludes_children(&s).await;
        }

        // Unspent by prefix
        #[tokio::test]
        async fn unspent_prefix_empty() {
            let s = $constructor.await;
            $crate::storage::test_support::unspent_prefix_empty(&s).await;
        }
        #[tokio::test]
        async fn unspent_prefix_includes_descendants() {
            let s = $constructor.await;
            $crate::storage::test_support::unspent_prefix_includes_descendants(&s).await;
        }
        #[tokio::test]
        async fn unspent_prefix_includes_exact() {
            let s = $constructor.await;
            $crate::storage::test_support::unspent_prefix_includes_exact(&s).await;
        }
        #[tokio::test]
        async fn unspent_prefix_excludes_spent() {
            let s = $constructor.await;
            $crate::storage::test_support::unspent_prefix_excludes_spent(&s).await;
        }
        #[tokio::test]
        async fn unspent_prefix_excludes_other_assets() {
            let s = $constructor.await;
            $crate::storage::test_support::unspent_prefix_excludes_other_assets(&s).await;
        }
        #[tokio::test]
        async fn unspent_prefix_excludes_non_descendants() {
            let s = $constructor.await;
            $crate::storage::test_support::unspent_prefix_excludes_non_descendants(&s).await;
        }

        // Transactions
        #[tokio::test]
        async fn load_transactions_empty() {
            let s = $constructor.await;
            $crate::storage::test_support::load_transactions_empty(&s).await;
        }
        #[tokio::test]
        async fn tx_count_empty() {
            let s = $constructor.await;
            $crate::storage::test_support::tx_count_empty(&s).await;
        }
        #[tokio::test]
        async fn commit_and_load_transaction() {
            let s = $constructor.await;
            $crate::storage::test_support::commit_and_load_transaction(&s).await;
        }
        #[tokio::test]
        async fn transactions_preserve_order() {
            let s = $constructor.await;
            $crate::storage::test_support::transactions_preserve_order(&s).await;
        }

        // Unspent all by prefix
        #[tokio::test]
        async fn unspent_all_prefix_empty() {
            let s = $constructor.await;
            $crate::storage::test_support::unspent_all_prefix_empty(&s).await;
        }
        #[tokio::test]
        async fn unspent_all_prefix_returns_multiple_assets() {
            let s = $constructor.await;
            $crate::storage::test_support::unspent_all_prefix_returns_multiple_assets(&s).await;
        }
        #[tokio::test]
        async fn unspent_all_prefix_excludes_spent() {
            let s = $constructor.await;
            $crate::storage::test_support::unspent_all_prefix_excludes_spent(&s).await;
        }
        #[tokio::test]
        async fn unspent_all_prefix_excludes_non_descendants() {
            let s = $constructor.await;
            $crate::storage::test_support::unspent_all_prefix_excludes_non_descendants(&s).await;
        }

        // Balances by prefix
        #[tokio::test]
        async fn balances_prefix_empty() {
            let s = $constructor.await;
            $crate::storage::test_support::balances_prefix_empty(&s).await;
        }
        #[tokio::test]
        async fn balances_prefix_groups_by_account_and_asset() {
            let s = $constructor.await;
            $crate::storage::test_support::balances_prefix_groups_by_account_and_asset(&s).await;
        }
        #[tokio::test]
        async fn balances_prefix_sums_multiple_tokens() {
            let s = $constructor.await;
            $crate::storage::test_support::balances_prefix_sums_multiple_tokens(&s).await;
        }
        #[tokio::test]
        async fn balances_prefix_excludes_spent() {
            let s = $constructor.await;
            $crate::storage::test_support::balances_prefix_excludes_spent(&s).await;
        }
        #[tokio::test]
        async fn balances_prefix_excludes_non_descendants() {
            let s = $constructor.await;
            $crate::storage::test_support::balances_prefix_excludes_non_descendants(&s).await;
        }
        #[tokio::test]
        async fn balances_prefix_omits_zero_balances() {
            let s = $constructor.await;
            $crate::storage::test_support::balances_prefix_omits_zero_balances(&s).await;
        }

        // Combined
        #[tokio::test]
        async fn commit_creates_tokens_and_key() {
            let s = $constructor.await;
            $crate::storage::test_support::commit_creates_tokens_and_key(&s).await;
        }
        #[tokio::test]
        async fn commit_spends_and_creates() {
            let s = $constructor.await;
            $crate::storage::test_support::commit_spends_and_creates(&s).await;
        }
    };
}
