use super::{Db, Tx};
use crate::amount::Amount;
use crate::error::AppError;
use crate::models::inventory::*;

impl Db {
    pub async fn list_receipts(&self) -> Result<Vec<InventoryReceipt>, AppError> {
        let receipts = sqlx::query_as::<_, InventoryReceipt>(
            "SELECT * FROM inventory_receipts ORDER BY received_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(receipts)
    }

    pub async fn get_receipt(&self, id: i64) -> Result<InventoryReceipt, AppError> {
        sqlx::query_as::<_, InventoryReceipt>("SELECT * FROM inventory_receipts WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Receipt not found".into()))
    }

    pub async fn get_receipt_lines(&self, receipt_id: i64) -> Result<Vec<ReceiptLine>, AppError> {
        let lines = sqlx::query_as::<_, ReceiptLine>(
            "SELECT * FROM inventory_receipt_lines WHERE receipt_id = ?",
        )
        .bind(receipt_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(lines)
    }

    pub async fn get_receipt_prices(&self, receipt_id: i64) -> Result<Vec<ReceiptPrice>, AppError> {
        let prices = sqlx::query_as::<_, ReceiptPrice>(
            "SELECT * FROM inventory_receipt_prices WHERE receipt_id = ?",
        )
        .bind(receipt_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(prices)
    }

    pub async fn get_supplier_entries(
        &self,
        receipt_id: i64,
    ) -> Result<Vec<SupplierLedgerUtxo>, AppError> {
        let entries = sqlx::query_as::<_, SupplierLedgerUtxo>(
            "SELECT * FROM supplier_ledger_utxos WHERE receipt_id = ? ORDER BY id ASC",
        )
        .bind(receipt_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(entries)
    }

    /// Get stock levels from the ledger.
    pub async fn stock_levels(
        &self,
        product_id: Option<i64>,
        warehouse_id: Option<i64>,
    ) -> Result<Vec<StockLevel>, AppError> {
        let accounts = self
            .ledger
            .accounts()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        // Filter to store/{wh_id} accounts, optionally by warehouse_id
        let store_accounts: Vec<(String, i64)> = accounts
            .into_iter()
            .filter_map(|acct| {
                let parts: Vec<&str> = acct.split('/').collect();
                if parts.len() != 2 || parts[0] != "store" {
                    return None;
                }
                let wh_id: i64 = parts[1].parse().ok()?;
                if let Some(filter_wid) = warehouse_id {
                    if wh_id != filter_wid {
                        return None;
                    }
                }
                Some((acct, wh_id))
            })
            .collect();

        let mut stock = Vec::new();

        for (account, wh_id) in store_accounts {
            let tokens = self
                .ledger
                .unspent_tokens(&account, None)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;

            // Group by asset, sum raw values
            let mut by_asset: std::collections::HashMap<String, (i128, u8)> =
                std::collections::HashMap::new();
            for token in &tokens {
                let name = token.amount.asset_name().to_string();
                if !name.starts_with("product:") {
                    continue;
                }
                let entry = by_asset
                    .entry(name)
                    .or_insert((0, token.amount.asset().precision()));
                entry.0 += token.amount.raw();
            }

            for (asset_name, (raw, precision)) in by_asset {
                let pid: i64 = match asset_name.strip_prefix("product:") {
                    Some(s) => match s.parse() {
                        Ok(v) => v,
                        Err(_) => continue,
                    },
                    None => continue,
                };
                if let Some(filter_pid) = product_id {
                    if pid != filter_pid {
                        continue;
                    }
                }

                let divisor = 10_f64.powi(precision as i32);
                let total_quantity = raw as f64 / divisor;

                stock.push(StockLevel {
                    product_id: pid,
                    warehouse_id: wh_id,
                    total_quantity,
                });
            }
        }

        Ok(stock)
    }

    /// Get supplier balance from unspent ledger tokens.
    pub async fn supplier_balance(&self) -> Result<SupplierBalance, AppError> {
        let accounts = self
            .ledger
            .accounts()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let payable_accounts: Vec<String> = accounts
            .into_iter()
            .filter(|a| a.starts_with("warehouse/payables/"))
            .collect();

        let mut total_owed: i64 = 0;
        let mut total_paid_offset: i64 = 0;

        for account in &payable_accounts {
            let tokens = self
                .ledger
                .unspent_tokens(account, None)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;

            for t in &tokens {
                if t.amount.asset_name() != "gs" {
                    continue;
                }
                let raw = t.amount.raw() as i64;
                if raw > 0 {
                    total_owed += raw;
                } else {
                    total_paid_offset += raw;
                }
            }
        }

        let outstanding = total_owed + total_paid_offset;

        Ok(SupplierBalance {
            total_owed: Amount(total_owed),
            total_paid: Amount(-total_paid_offset),
            outstanding: Amount(outstanding),
        })
    }

    /// Get the payable balance for a specific receipt from the ledger.
    pub async fn receipt_outstanding(&self, receipt_id: i64) -> Result<i128, AppError> {
        let payable_account = format!("warehouse/payables/{receipt_id}");
        self.ledger
            .balance(&payable_account, "gs")
            .await
            .map_err(|e| AppError::Internal(e.to_string()))
    }

    /// List all ledger transactions (used for transfer history).
    pub async fn ledger_transactions(&self) -> Result<Vec<ledger::Transaction>, AppError> {
        self.ledger
            .transactions()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))
    }
}

impl Tx {
    /// Receive inventory: insert receipt metadata + ledger issue.
    pub async fn receive_inventory(
        &mut self,
        body: &ReceiveInventoryRequest,
    ) -> Result<InventoryReceipt, AppError> {
        let total_cost: i64 = body
            .lines
            .iter()
            .map(|l| (l.quantity * l.cost_per_unit.cents() as f64).round() as i64)
            .sum();

        let receipt = sqlx::query_as::<_, InventoryReceipt>(
            "INSERT INTO inventory_receipts (reference, supplier_name, notes, total_cost)
             VALUES (?, ?, ?, ?) RETURNING *",
        )
        .bind(&body.reference)
        .bind(&body.supplier_name)
        .bind(&body.notes)
        .bind(total_cost)
        .fetch_one(&mut *self.inner)
        .await?;

        let mut builder = self.ledger.transaction(format!("receipt-{}", receipt.id));

        for line in &body.lines {
            if line.quantity <= 0.0 {
                return Err(AppError::BadRequest("Quantity must be positive".into()));
            }

            sqlx::query(
                "INSERT INTO inventory_receipt_lines (receipt_id, product_id, warehouse_id, quantity, cost_per_unit)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(receipt.id)
            .bind(line.product_id)
            .bind(line.warehouse_id)
            .bind(line.quantity)
            .bind(line.cost_per_unit)
            .execute(&mut *self.inner)
            .await?;

            let account = format!("warehouse/{}", line.warehouse_id);
            let asset = self
                .ledger
                .asset(&format!("product:{}", line.product_id))
                .ok_or_else(|| {
                    AppError::Internal(format!("asset product:{} not registered", line.product_id))
                })?;
            let amount = asset
                .parse_amount(&format!("{:.3}", line.quantity))
                .map_err(|e| AppError::Internal(format!("parse amount: {e}")))?;
            builder = builder
                .issue(&account, &amount)
                .map_err(|e| AppError::Internal(format!("issue: {e}")))?;

            for price in &line.prices {
                sqlx::query(
                    "INSERT INTO inventory_receipt_prices (receipt_id, product_id, customer_group_id, price_per_unit)
                     VALUES (?, ?, ?, ?)",
                )
                .bind(receipt.id)
                .bind(line.product_id)
                .bind(price.customer_group_id)
                .bind(price.price_per_unit)
                .execute(&mut *self.inner)
                .await?;
            }
        }

        // Supplier debt
        let is_credit = body.is_credit.unwrap_or(false);
        let paid_cash = body.paid_cash.unwrap_or(false);

        if is_credit || paid_cash {
            let gs = self
                .ledger
                .asset("gs")
                .ok_or_else(|| AppError::Internal("gs asset not registered".into()))?;
            let gs_amount = gs.try_amount(total_cost as i128);
            let neg_gs_amount = gs.try_amount(-(total_cost as i128));

            builder = builder
                .credit(&format!("supplier/{}", receipt.id), &gs_amount)
                .credit(
                    &format!("warehouse/payables/{}", receipt.id),
                    &neg_gs_amount,
                );

            sqlx::query(
                "INSERT INTO supplier_ledger_utxos (receipt_id, amount, method, notes) VALUES (?, ?, ?, ?)",
            )
            .bind(receipt.id)
            .bind(-total_cost)
            .bind::<Option<String>>(None)
            .bind("Inventory received")
            .execute(&mut *self.inner)
            .await?;

            if paid_cash {
                builder = builder
                    .credit(&format!("supplier/{}", receipt.id), &neg_gs_amount)
                    .credit(
                        &format!("warehouse/payables/{}", receipt.id),
                        &neg_gs_amount,
                    );

                sqlx::query(
                    "INSERT INTO supplier_ledger_utxos (receipt_id, amount, method, notes) VALUES (?, ?, ?, ?)",
                )
                .bind(receipt.id)
                .bind(total_cost)
                .bind("cash")
                .bind("Paid in cash")
                .execute(&mut *self.inner)
                .await?;
            }
        }

        let ledger_tx = builder
            .build()
            .await
            .map_err(|e| AppError::Internal(format!("ledger build: {e}")))?;
        self.ledger
            .commit(ledger_tx)
            .await
            .map_err(|e| AppError::Internal(format!("ledger commit: {e}")))?;

        Ok(receipt)
    }

    /// Record a supplier payment (SQL + ledger).
    pub async fn record_supplier_payment(
        &mut self,
        receipt_id: i64,
        body: &CreateSupplierPayment,
    ) -> Result<SupplierLedgerUtxo, AppError> {
        let entry = sqlx::query_as::<_, SupplierLedgerUtxo>(
            "INSERT INTO supplier_ledger_utxos (receipt_id, amount, method, notes) VALUES (?, ?, ?, ?) RETURNING *",
        )
        .bind(receipt_id)
        .bind(body.amount)
        .bind(&body.method)
        .bind(&body.notes)
        .fetch_one(&mut *self.inner)
        .await?;

        let amount_cents = body.amount.cents();
        let gs = self
            .ledger
            .asset("gs")
            .ok_or_else(|| AppError::Internal("gs asset not registered".into()))?;
        let gs_amount = gs.try_amount(amount_cents as i128);
        let neg_gs_amount = gs.try_amount(-(amount_cents as i128));

        let ledger_tx = self
            .ledger
            .transaction(format!("supplier-payment-{}", entry.id))
            .credit(&format!("supplier/{receipt_id}"), &gs_amount)
            .credit(&format!("warehouse/payables/{receipt_id}"), &neg_gs_amount)
            .build()
            .await
            .map_err(|e| AppError::Internal(format!("ledger build: {e}")))?;
        self.ledger
            .commit(ledger_tx)
            .await
            .map_err(|e| AppError::Internal(format!("ledger commit: {e}")))?;

        Ok(entry)
    }

    /// Transfer inventory between warehouses (ledger only, but validates warehouses via SQL).
    pub async fn transfer_inventory(
        &mut self,
        body: &TransferInventoryRequest,
    ) -> Result<String, AppError> {
        if body.from_warehouse_id == body.to_warehouse_id {
            return Err(AppError::BadRequest(
                "Source and destination warehouse must be different".into(),
            ));
        }
        if body.lines.is_empty() {
            return Err(AppError::BadRequest("At least one line is required".into()));
        }

        // Verify warehouses exist
        let from_wh: i64 = sqlx::query_scalar("SELECT id FROM warehouses WHERE id = ?")
            .bind(body.from_warehouse_id)
            .fetch_optional(&mut *self.inner)
            .await?
            .ok_or_else(|| AppError::NotFound("Source warehouse not found".into()))?;

        let to_wh: i64 = sqlx::query_scalar("SELECT id FROM warehouses WHERE id = ?")
            .bind(body.to_warehouse_id)
            .fetch_optional(&mut *self.inner)
            .await?
            .ok_or_else(|| AppError::NotFound("Destination warehouse not found".into()))?;

        let tx_id = format!(
            "transfer-{}-{}-{}",
            from_wh,
            to_wh,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
        let mut builder = self.ledger.transaction(&tx_id);

        for line in &body.lines {
            if line.quantity <= 0.0 {
                return Err(AppError::BadRequest("Quantity must be positive".into()));
            }

            let asset = self
                .ledger
                .asset(&format!("product:{}", line.product_id))
                .ok_or_else(|| {
                    AppError::Internal(format!("asset product:{} not registered", line.product_id))
                })?;
            let amount = asset
                .parse_amount(&format!("{:.3}", line.quantity))
                .map_err(|e| AppError::Internal(format!("parse amount: {e}")))?;

            let from_account = format!("warehouse/{}", from_wh);
            let to_account = format!("warehouse/{}", to_wh);

            builder = builder
                .debit(&from_account, &amount)
                .credit(&to_account, &amount);
        }

        let ledger_tx = builder
            .build()
            .await
            .map_err(|e| AppError::Internal(format!("ledger build: {e}")))?;
        self.ledger
            .commit(ledger_tx)
            .await
            .map_err(|e| AppError::Internal(format!("ledger commit: {e}")))?;

        Ok(tx_id)
    }
}
