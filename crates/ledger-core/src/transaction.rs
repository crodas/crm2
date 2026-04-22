//! Transaction types and builder.
//!
//! A transaction is an atomic state transition that consumes debit spending
//! tokens and produces credit spending tokens. Transactions are append-only:
//! once committed, they are never modified.
//!
//! ## Transaction ID derivation
//!
//! The `tx_id` is deterministically computed:
//!
//! ```text
//! tx_id = hex(sha256(sha256(canonical_preimage)))
//! ```
//!
//! The canonical preimage is a null-byte (`\0`) delimited concatenation of
//! all debits, credits, and the idempotency key. Declaration order is
//! preserved — the caller is responsible for deterministic construction.
//!
//! The double SHA-256 guards against length-extension attacks on the outer hash.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::account::AccountPath;
use crate::amount::Amount;
use crate::error::LedgerError;

/// A reference to a prior credit being consumed as a debit.
///
/// The caller must supply all fields; the engine verifies them against the
/// actual stored token at commit time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DebitRef {
    /// Transaction that created the token being spent.
    pub tx_id: String,
    /// Position within that transaction's credits.
    pub entry_index: u32,
    /// Expected owner of the token (verified at commit time).
    pub from: AccountPath,
    /// Expected amount (verified at commit time).
    pub amount: Amount,
}

/// A new credit to be created by the transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credit {
    /// Destination account path.
    pub to: AccountPath,
    /// Amount to credit.
    pub amount: Amount,
}

/// A committed transaction in the ledger.
///
/// Guaranteed to be balanced at construction time: credit and debit sums
/// match per asset, no dangling debt, no negative unsigned quantities.
///
/// The `tx_id` is derived from the canonical preimage of debits, credits, and
/// the idempotency key. It is never supplied by the caller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    /// Derived transaction ID: `hex(sha256(sha256(preimage)))`.
    pub tx_id: String,
    /// Caller-supplied unique key for idempotent submission.
    pub idempotency_key: String,
    /// Spending tokens consumed (empty for issuance from `@world`).
    pub debits: Vec<DebitRef>,
    /// New spending tokens produced.
    pub credits: Vec<Credit>,
}

/// Builder for constructing validated transactions.
///
/// ```
/// # use ledger_core::*;
/// let brush = Asset::new("brush", 0, AssetKind::Unsigned);
/// let five = brush.try_amount(5).unwrap();
///
/// let tx = TransactionBuilder::new("issue-brush-001")
///     .credit("@store1/inventory", &five).unwrap()
///     .build()
///     .unwrap();
///
/// assert!(tx.debits.is_empty()); // issuance from @world
/// assert_eq!(tx.credits.len(), 1);
/// ```
pub struct TransactionBuilder {
    idempotency_key: String,
    debits: Vec<DebitRef>,
    credits: Vec<Credit>,
}

impl TransactionBuilder {
    /// Start building a transaction with the given idempotency key.
    pub fn new(idempotency_key: impl Into<String>) -> Self {
        Self {
            idempotency_key: idempotency_key.into(),
            debits: Vec::new(),
            credits: Vec::new(),
        }
    }

    /// Add a debit (a prior credit to consume).
    pub fn debit(
        mut self,
        tx_id: impl Into<String>,
        entry_index: u32,
        from: impl Into<String>,
        amount: &Amount,
    ) -> Result<Self, LedgerError> {
        let from_str: String = from.into();
        let from = AccountPath::new(from_str.clone())
            .map_err(|_| LedgerError::InvalidAccount(from_str))?;
        self.debits.push(DebitRef {
            tx_id: tx_id.into(),
            entry_index,
            from,
            amount: amount.clone(),
        });
        Ok(self)
    }

    /// Add a credit (a new spending token to create).
    pub fn credit(mut self, to: impl Into<String>, amount: &Amount) -> Result<Self, LedgerError> {
        let to_str: String = to.into();
        if to_str == "@world" {
            return Err(LedgerError::WorldAsOwner);
        }
        let to =
            AccountPath::new(to_str.clone()).map_err(|_| LedgerError::InvalidAccount(to_str))?;
        self.credits.push(Credit {
            to,
            amount: amount.clone(),
        });
        Ok(self)
    }

    /// Build the transaction, validating balance invariants.
    ///
    /// Checks performed:
    /// - Unsigned assets cannot have negative quantities (enforced at Amount creation)
    /// - Conservation: per-asset debit sums == credit sums (non-issuance)
    /// - Dual-sided debt: negative credits balanced by positive credits
    ///
    /// On success returns a [`Transaction`] with a deterministic `tx_id`.
    /// Call [`Ledger::commit`] to validate against ledger state and append.
    pub fn build(self) -> Result<Transaction, LedgerError> {
        let tx_id = compute_tx_id(&self.debits, &self.credits, &self.idempotency_key);

        // Collect credit sums per asset.
        let mut asset_credit_sums: std::collections::HashMap<&str, i128> = HashMap::new();
        for credit in &self.credits {
            *asset_credit_sums
                .entry(credit.amount.asset_name())
                .or_default() += credit.amount.raw();
        }

        // Collect debit sums per asset.
        let mut asset_debit_sums: std::collections::HashMap<&str, i128> = HashMap::new();
        for debit in &self.debits {
            *asset_debit_sums
                .entry(debit.amount.asset_name())
                .or_default() += debit.amount.raw();
        }

        // Conservation: per-asset debit sum == credit sum (skip for issuance).
        if !self.debits.is_empty() {
            let all_assets: HashSet<&str> = asset_debit_sums
                .keys()
                .chain(asset_credit_sums.keys())
                .copied()
                .collect();

            for asset in all_assets {
                let d = asset_debit_sums.get(asset).unwrap_or(&0);
                let c = asset_credit_sums.get(asset).unwrap_or(&0);
                if d != c {
                    return Err(LedgerError::ConservationViolated {
                        asset: asset.to_string(),
                        debit_sum: *d,
                        credit_sum: *c,
                    });
                }
            }
        }

        // Dual-sided debt: negative credits must be balanced by positive credits.
        let mut neg_debt_by_asset: std::collections::HashMap<&str, i128> = HashMap::new();
        let mut pos_debt_by_asset: std::collections::HashMap<&str, i128> = HashMap::new();

        for credit in &self.credits {
            let qty = credit.amount.raw();
            let asset_name = credit.amount.asset_name();
            if qty < 0 {
                *neg_debt_by_asset.entry(asset_name).or_default() += qty;
            } else if qty > 0 {
                *pos_debt_by_asset.entry(asset_name).or_default() += qty;
            }
        }

        for (asset_name, neg_sum) in &neg_debt_by_asset {
            let pos_sum = pos_debt_by_asset.get(asset_name).unwrap_or(&0);
            if *pos_sum < neg_sum.unsigned_abs() as i128 {
                return Err(LedgerError::DanglingDebt {
                    asset: asset_name.to_string(),
                });
            }
        }

        Ok(Transaction {
            tx_id,
            idempotency_key: self.idempotency_key,
            debits: self.debits,
            credits: self.credits,
        })
    }
}

/// Compute the deterministic transaction ID.
///
/// ```text
/// tx_id = hex(sha256(sha256(preimage)))
/// ```
///
/// Debits and credits preserve declaration order — the caller is responsible
/// for constructing transactions deterministically.
///
/// The preimage is a null-byte (`\0`) delimited concatenation:
///
/// ```text
/// D\0<tx_id>\0<entry_index>\0<owner>\0<asset>\0<qty>\0
/// ...
/// C\0<to>\0<asset>\0<qty>\0
/// ...
/// K\0<idempotency_key>
/// ```
pub fn compute_tx_id(debits: &[DebitRef], credits: &[Credit], idempotency_key: &str) -> String {
    let preimage = canonical_preimage(debits, credits, idempotency_key);
    let first = Sha256::digest(preimage.as_bytes());
    let second = Sha256::digest(first);
    hex::encode(second)
}

/// Produce the canonical preimage for transaction ID derivation.
///
/// Uses null-byte delimiters with tagged sections (D for debit, C for credit,
/// K for key). Declaration order is preserved for both debits and credits.
fn canonical_preimage(debits: &[DebitRef], credits: &[Credit], idempotency_key: &str) -> String {
    let mut out = String::new();

    for d in debits {
        out.push_str("D\0");
        out.push_str(&d.tx_id);
        out.push('\0');
        out.push_str(&d.entry_index.to_string());
        out.push('\0');
        out.push_str(d.from.as_str());
        out.push('\0');
        out.push_str(d.amount.asset_name());
        out.push('\0');
        out.push_str(&d.amount.to_decimal_string());
        out.push('\0');
    }

    for c in credits {
        out.push_str("C\0");
        out.push_str(c.to.as_str());
        out.push('\0');
        out.push_str(c.amount.asset_name());
        out.push('\0');
        out.push_str(&c.amount.to_decimal_string());
        out.push('\0');
    }

    out.push_str("K\0");
    out.push_str(idempotency_key);

    out
}

/// Inline hex encoding (avoids adding the `hex` crate dependency).
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::{Asset, AssetKind};

    fn brush() -> Asset {
        Asset::new("brush", 0, AssetKind::Unsigned)
    }

    fn usd() -> Asset {
        Asset::new("usd", 2, AssetKind::Signed)
    }

    #[test]
    fn issuance_tx_id_is_deterministic() -> Result<(), LedgerError> {
        let five_brush = brush().try_amount(5).unwrap();
        let tx1 = TransactionBuilder::new("test-key")
            .credit("@store1/inventory", &five_brush)?
            .build()
            .expect("build issuance tx1");
        let tx2 = TransactionBuilder::new("test-key")
            .credit("@store1/inventory", &five_brush)?
            .build()
            .expect("build issuance tx2");
        assert_eq!(tx1.tx_id, tx2.tx_id);
        assert_eq!(tx1.tx_id.len(), 64);
        Ok(())
    }

    #[test]
    fn different_keys_produce_different_ids() -> Result<(), LedgerError> {
        let five_brush = brush().try_amount(5).unwrap();
        let tx1 = TransactionBuilder::new("key-a")
            .credit("@store1/inventory", &five_brush)?
            .build()
            .expect("build tx with key-a");
        let tx2 = TransactionBuilder::new("key-b")
            .credit("@store1/inventory", &five_brush)?
            .build()
            .expect("build tx with key-b");
        assert_ne!(tx1.tx_id, tx2.tx_id);
        Ok(())
    }

    #[test]
    fn debit_order_matters_for_tx_id() -> Result<(), LedgerError> {
        let usd1 = usd().try_amount(100).unwrap();
        let usd2 = usd().try_amount(200).unwrap();
        let usd3 = usd().try_amount(300).unwrap();
        let tx1 = TransactionBuilder::new("k")
            .debit("aaa", 0, "@x", &usd1)?
            .debit("bbb", 0, "@y", &usd2)?
            .credit("@z", &usd3)?
            .build()
            .expect("build tx with debit order a,b");
        let tx2 = TransactionBuilder::new("k")
            .debit("bbb", 0, "@y", &usd2)?
            .debit("aaa", 0, "@x", &usd1)?
            .credit("@z", &usd3)?
            .build()
            .expect("build tx with debit order b,a");
        assert_ne!(tx1.tx_id, tx2.tx_id);
        Ok(())
    }

    #[test]
    fn canonical_preimage_format() {
        let preimage = canonical_preimage(&[], &[], "k");
        assert_eq!(preimage, "K\0k");
    }

    #[test]
    fn conservation_rejected_at_build() -> Result<(), LedgerError> {
        let five_brush = brush().try_amount(5).unwrap();
        let ten_brush = brush().try_amount(10).unwrap();
        let result = TransactionBuilder::new("bad")
            .debit("aaa", 0, "@x", &five_brush)?
            .credit("@y", &ten_brush)?
            .build();
        assert!(matches!(
            result,
            Err(LedgerError::ConservationViolated { .. })
        ));
        Ok(())
    }

    #[test]
    fn dangling_debt_rejected_at_build() -> Result<(), LedgerError> {
        let neg_usd = usd().try_amount(-1000).unwrap();
        let result = TransactionBuilder::new("bad")
            .credit("@x", &neg_usd)?
            .build();
        assert!(matches!(result, Err(LedgerError::DanglingDebt { .. })));
        Ok(())
    }

    #[test]
    fn negative_unsigned_rejected_at_amount_creation() {
        // This now fails at Amount creation, not at build time.
        assert!(brush().try_amount(-5).is_err());
    }

    #[test]
    fn world_as_owner_rejected() {
        let five_brush = brush().try_amount(5).unwrap();
        let result = TransactionBuilder::new("bad").credit("@world", &five_brush);
        assert!(matches!(result, Err(LedgerError::WorldAsOwner)));
    }
}
