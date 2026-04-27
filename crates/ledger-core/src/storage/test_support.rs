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
    Asset, CreditEntryRef, CreditToken, LedgerError, Storage, CreditTokenStatus, Transaction,
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
) -> (Transaction, Vec<CreditToken>) {
    let amount = asset.try_amount(raw).expect("valid amount for fixture");
    let neg = amount.negate();
    let tx = TransactionBuilder::new(key)
        .credit(account, &amount)
        .credit("@world", &neg)
        .build()
        .expect("build issuance fixture");

    let tokens = vec![CreditToken {
        entry_ref: CreditEntryRef {
            tx_id: tx.tx_id.clone(),
            entry_index: 0,
        },
        owner: account.to_string(),
        amount,
        status: CreditTokenStatus::Unspent,
    }];

    (tx, tokens)
}

/// Build a transfer that spends one credit token and creates a new one.
fn make_transfer(
    key: &str,
    spent_ref: &CreditEntryRef,
    from: &str,
    to: &str,
    asset: &Asset,
    raw: i128,
) -> (Transaction, Vec<CreditToken>, Vec<CreditEntryRef>) {
    let amount = asset.try_amount(raw).expect("valid amount for fixture");
    let tx = TransactionBuilder::new(key)
        .debit(&spent_ref.tx_id, spent_ref.entry_index, from, &amount)
        .credit(to, &amount)
        .build()
        .expect("build transfer fixture");

    let new_tokens = vec![CreditToken {
        entry_ref: CreditEntryRef {
            tx_id: tx.tx_id.clone(),
            entry_index: 0,
        },
        owner: to.to_string(),
        amount,
        status: CreditTokenStatus::Unspent,
    }];

    let spent = vec![spent_ref.clone()];
    (tx, new_tokens, spent)
}

/// Commit a full transaction using the granular storage primitives.
async fn commit(
    s: &dyn Storage,
    tx: &Transaction,
    tokens: &[CreditToken],
    spent: &[CreditEntryRef],
) {
    if !spent.is_empty() {
        s.mark_spent(spent, &tx.tx_id).await.expect("mark_spent");
    }
    s.insert_credit_tokens(tokens).await.expect("insert_tokens");
    s.insert_tx(tx).await.expect("insert_tx");
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
    commit(s, &tx, &tokens, &[]).await;
    assert!(s.has_idempotency_key("issue-001").await.expect("has_key"));
}

pub async fn key_absent_for_uncommitted(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx, tokens) = make_issuance("issue-001", "a", &brush(), 5);
    commit(s, &tx, &tokens, &[]).await;
    assert!(!s.has_idempotency_key("other-key").await.expect("has_key"));
}

// ── Credit token tests ───────────────────────────────────────────────

pub async fn get_token_nonexistent(s: &dyn Storage) {
    let eref = CreditEntryRef {
        tx_id: "nonexistent".to_string(),
        entry_index: 0,
    };
    assert!(s.get_credit_token(&eref).await.expect("get_token").is_none());
}

pub async fn get_token_after_commit(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx, tokens) = make_issuance("issue-001", "a", &brush(), 5);
    let eref = tokens[0].entry_ref.clone();
    commit(s, &tx, &tokens, &[]).await;

    let credit = s
        .get_credit_token(&eref)
        .await
        .expect("get_credit_token")
        .expect("credit token should exist");
    assert_eq!(credit.amount.raw(), 5);
    assert_eq!(credit.amount.asset_name(), "brush");
    assert_eq!(credit.status, CreditTokenStatus::Unspent);
}

pub async fn token_marked_spent(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx1, tokens1) = make_issuance("issue-001", "a", &brush(), 5);
    let eref = tokens1[0].entry_ref.clone();
    commit(s, &tx1, &tokens1, &[]).await;

    let (tx2, tokens2, spent) = make_transfer("xfer-001", &eref, "a", "b", &brush(), 5);
    commit(s, &tx2, &tokens2, &spent).await;

    let credit = s
        .get_credit_token(&eref)
        .await
        .expect("get_credit_token")
        .expect("credit token should exist");
    assert!(matches!(credit.status, CreditTokenStatus::Spent(_)));
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
    commit(s, &tx, &tokens, &[]).await;

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
    commit(s, &tx1, &tokens1, &[]).await;

    let (tx2, tokens2, spent) = make_transfer("xfer-001", &eref, "a", "b", &brush(), 5);
    commit(s, &tx2, &tokens2, &spent).await;

    let result = s
        .unspent_by_account("a", Some(&brush().max()))
        .await
        .expect("unspent_by_account");
    assert!(result.is_empty(), "spent tokens should be excluded");
}

pub async fn unspent_account_excludes_other_assets(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx, tokens) = make_issuance("issue-001", "a", &brush(), 5);
    commit(s, &tx, &tokens, &[]).await;

    let result = s
        .unspent_by_account("a", Some(&usd().max()))
        .await
        .expect("unspent_by_account");
    assert!(result.is_empty(), "different asset should be excluded");
}

pub async fn unspent_account_excludes_children(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx, tokens) = make_issuance("issue-001", "store1/inventory", &brush(), 5);
    commit(s, &tx, &tokens, &[]).await;

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
    commit(s, &tx, &tokens, &[]).await;

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
    commit(s, &tx, &tokens, &[]).await;

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
    commit(s, &tx1, &tokens1, &[]).await;

    let (tx2, tokens2, spent) =
        make_transfer("xfer-001", &eref, "store1/inventory", "b", &brush(), 5);
    commit(s, &tx2, &tokens2, &spent).await;

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
    commit(s, &tx, &tokens, &[]).await;

    let result = s
        .unspent_by_prefix("store1", Some(&usd().max()))
        .await
        .expect("unspent_by_prefix");
    assert!(result.is_empty(), "different asset should be excluded");
}

pub async fn unspent_prefix_excludes_non_descendants(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx, tokens) = make_issuance("issue-001", "store2", &brush(), 5);
    commit(s, &tx, &tokens, &[]).await;

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
    commit(s, &tx, &tokens, &[]).await;

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

    commit(s, &tx1, &tokens1, &[]).await;
    commit(s, &tx2, &tokens2, &[]).await;

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
    commit(s, &tx, &tokens, &[]).await;

    let credit = s
        .get_credit_token(&eref)
        .await
        .expect("get_credit_token")
        .expect("credit token should exist");
    assert_eq!(credit.amount.raw(), 5);
    assert!(s.has_idempotency_key("issue-001").await.expect("has_key"));
    assert_eq!(s.tx_count().await.expect("tx_count"), 1);
}

pub async fn commit_spends_and_creates(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx1, tokens1) = make_issuance("issue-001", "a", &brush(), 5);
    let eref_a = tokens1[0].entry_ref.clone();
    commit(s, &tx1, &tokens1, &[]).await;

    let (tx2, tokens2, spent) = make_transfer("xfer-001", &eref_a, "a", "b", &brush(), 5);
    let eref_b = tokens2[0].entry_ref.clone();
    commit(s, &tx2, &tokens2, &spent).await;

    let a = s
        .get_credit_token(&eref_a)
        .await
        .expect("get_token A")
        .expect("A should exist");
    assert!(matches!(a.status, CreditTokenStatus::Spent(_)));

    let b = s
        .get_credit_token(&eref_b)
        .await
        .expect("get_token B")
        .expect("B should exist");
    assert_eq!(b.status, CreditTokenStatus::Unspent);

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
    commit(s, &tx1, &tokens1, &[]).await;

    let (tx2, tokens2) = make_issuance("issue-002", "store1/cash", &usd(), 1000);
    commit(s, &tx2, &tokens2, &[]).await;

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
    commit(s, &tx1, &tokens1, &[]).await;

    let (tx2, tokens2, spent) = make_transfer("xfer-001", &eref, "a", "b", &brush(), 5);
    commit(s, &tx2, &tokens2, &spent).await;

    let result = s
        .unspent_by_prefix("a", None)
        .await
        .expect("unspent_by_prefix");
    assert!(result.is_empty());
}

pub async fn unspent_all_prefix_excludes_non_descendants(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx, tokens) = make_issuance("issue-001", "store2", &brush(), 5);
    commit(s, &tx, &tokens, &[]).await;

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
    commit(s, &tx1, &tokens1, &[]).await;

    let (tx2, tokens2) = make_issuance("issue-002", "store/w2/product/1", &brush(), 3);
    commit(s, &tx2, &tokens2, &[]).await;

    let (tx3, tokens3) = make_issuance("issue-003", "store/w1/product/1", &usd(), 1000);
    commit(s, &tx3, &tokens3, &[]).await;

    let result = s
        .balances_by_prefix("store")
        .await
        .expect("balances_by_prefix");

    // Should have 3 (account, asset) pairs across all accounts
    let total: usize = result.values().map(|m| m.len()).sum();
    assert_eq!(total, 3);

    assert_eq!(result["store/w1/product/1"]["brush"].raw(), 5);
    assert_eq!(result["store/w2/product/1"]["brush"].raw(), 3);
    assert_eq!(result["store/w1/product/1"]["usd"].raw(), 1000);
}

pub async fn balances_prefix_sums_multiple_tokens(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx1, tokens1) = make_issuance("issue-001", "a/sub", &brush(), 5);
    commit(s, &tx1, &tokens1, &[]).await;

    let (tx2, tokens2) = make_issuance("issue-002", "a/sub", &brush(), 3);
    commit(s, &tx2, &tokens2, &[]).await;

    let result = s.balances_by_prefix("a").await.expect("balances_by_prefix");
    assert_eq!(result["a/sub"]["brush"].raw(), 8);
}

pub async fn balances_prefix_excludes_spent(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx1, tokens1) = make_issuance("issue-001", "a", &brush(), 5);
    let eref = tokens1[0].entry_ref.clone();
    commit(s, &tx1, &tokens1, &[]).await;

    let (tx2, tokens2, spent) = make_transfer("xfer-001", &eref, "a", "b", &brush(), 5);
    commit(s, &tx2, &tokens2, &spent).await;

    let result = s.balances_by_prefix("a").await.expect("balances_by_prefix");
    assert!(
        result.is_empty(),
        "spent tokens should not contribute to balance"
    );
}

pub async fn balances_prefix_excludes_non_descendants(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx, tokens) = make_issuance("issue-001", "store2", &brush(), 5);
    commit(s, &tx, &tokens, &[]).await;

    let result = s
        .balances_by_prefix("store1")
        .await
        .expect("balances_by_prefix");
    assert!(result.is_empty());
}

pub async fn balances_prefix_omits_zero_balances(s: &dyn Storage) {
    register_test_assets(s).await;
    // Create and fully spend a credit token — net balance is 0
    let (tx1, tokens1) = make_issuance("issue-001", "a", &brush(), 5);
    let eref = tokens1[0].entry_ref.clone();
    commit(s, &tx1, &tokens1, &[]).await;

    // Transfer all to @a/sub (still under prefix @a)
    let (tx2, tokens2, spent) = make_transfer("xfer-001", &eref, "a", "a/sub", &brush(), 5);
    commit(s, &tx2, &tokens2, &spent).await;

    let result = s.balances_by_prefix("a").await.expect("balances_by_prefix");

    // @a has 0 balance (spent), @a/sub has 5 — only @a/sub should appear
    assert!(!result.contains_key("a"), "zero-balance account should be omitted");
    assert_eq!(result["a/sub"]["brush"].raw(), 5);
}

// ── Granular write primitive tests ──────────────────────────────────

pub async fn mark_spent_flags_tokens(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx, tokens) = make_issuance("issue-001", "a", &brush(), 5);
    let eref = tokens[0].entry_ref.clone();
    s.insert_credit_tokens(&tokens).await.expect("insert_tokens");
    s.insert_tx(&tx).await.expect("insert_tx");

    // Token starts unspent
    let credit = s.get_credit_token(&eref).await.unwrap().unwrap();
    assert_eq!(credit.status, CreditTokenStatus::Unspent);

    // Mark as spent
    s.mark_spent(&[eref.clone()], "some-tx")
        .await
        .expect("mark_spent");

    let credit = s.get_credit_token(&eref).await.unwrap().unwrap();
    assert!(matches!(credit.status, CreditTokenStatus::Spent(_)));
}

pub async fn unmark_spent_restores_tokens(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx, tokens) = make_issuance("issue-001", "a", &brush(), 5);
    let eref = tokens[0].entry_ref.clone();
    s.insert_credit_tokens(&tokens).await.expect("insert_tokens");
    s.insert_tx(&tx).await.expect("insert_tx");

    // Mark then unmark
    s.mark_spent(&[eref.clone()], "some-tx")
        .await
        .expect("mark_spent");
    s.unmark_spent(&[eref.clone()], "some-tx")
        .await
        .expect("unmark_spent");

    let credit = s.get_credit_token(&eref).await.unwrap().unwrap();
    assert_eq!(credit.status, CreditTokenStatus::Unspent);

    // Token should be visible in unspent queries again
    let unspent = s
        .unspent_by_account("a", Some(&brush().max()))
        .await
        .unwrap();
    assert_eq!(unspent.len(), 1);
}

pub async fn insert_and_remove_tokens(s: &dyn Storage) {
    register_test_assets(s).await;
    let (_, tokens) = make_issuance("issue-001", "a", &brush(), 5);
    let eref = tokens[0].entry_ref.clone();

    // Insert
    s.insert_credit_tokens(&tokens).await.expect("insert_tokens");
    let credit = s.get_credit_token(&eref).await.unwrap().unwrap();
    assert_eq!(credit.amount.raw(), 5);

    // Remove
    s.remove_credit_tokens(&[eref.clone()])
        .await
        .expect("remove_tokens");
    assert!(s.get_credit_token(&eref).await.unwrap().is_none());
}

pub async fn insert_and_remove_tx(s: &dyn Storage) {
    register_test_assets(s).await;
    let (tx, _tokens) = make_issuance("issue-001", "a", &brush(), 5);
    let tx_id = tx.tx_id.clone();

    // Insert
    s.insert_tx(&tx).await.expect("insert_tx");
    assert!(s.has_idempotency_key("issue-001").await.unwrap());
    assert_eq!(s.tx_count().await.unwrap(), 1);

    // Remove
    s.remove_tx(&tx_id).await.expect("remove_tx");
    assert!(!s.has_idempotency_key("issue-001").await.unwrap());
    assert_eq!(s.tx_count().await.unwrap(), 0);
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
        $crate::_storage_test_cases!($constructor;
            // Assets
            load_assets_empty,
            save_and_load_asset,
            register_asset_duplicate_is_noop,
            register_asset_conflict_rejected,
            save_multiple_assets,
            // Idempotency
            has_key_empty,
            key_recorded_after_commit,
            key_absent_for_uncommitted,
            // Tokens
            get_token_nonexistent,
            get_token_after_commit,
            token_marked_spent,
            // Unspent by account
            unspent_account_empty,
            unspent_account_returns_matching,
            unspent_account_excludes_spent,
            unspent_account_excludes_other_assets,
            unspent_account_excludes_children,
            // Unspent by prefix
            unspent_prefix_empty,
            unspent_prefix_includes_descendants,
            unspent_prefix_includes_exact,
            unspent_prefix_excludes_spent,
            unspent_prefix_excludes_other_assets,
            unspent_prefix_excludes_non_descendants,
            // Transactions
            load_transactions_empty,
            tx_count_empty,
            commit_and_load_transaction,
            transactions_preserve_order,
            // Unspent all by prefix
            unspent_all_prefix_empty,
            unspent_all_prefix_returns_multiple_assets,
            unspent_all_prefix_excludes_spent,
            unspent_all_prefix_excludes_non_descendants,
            // Balances by prefix
            balances_prefix_empty,
            balances_prefix_groups_by_account_and_asset,
            balances_prefix_sums_multiple_tokens,
            balances_prefix_excludes_spent,
            balances_prefix_excludes_non_descendants,
            balances_prefix_omits_zero_balances,
            // Combined
            commit_creates_tokens_and_key,
            commit_spends_and_creates,
            // Granular write primitives
            mark_spent_flags_tokens,
            unmark_spent_restores_tokens,
            insert_and_remove_tokens,
            insert_and_remove_tx,
        );
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! _storage_test_cases {
    ($constructor:expr; $($test_fn:ident),* $(,)?) => {
        $(
            #[tokio::test]
            async fn $test_fn() {
                let s = $constructor.await;
                $crate::storage::test_support::$test_fn(&s).await;
            }
        )*
    };
}
