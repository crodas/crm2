use super::{Db, Tx};
use crate::amount::Amount;
use crate::error::AppError;
use crate::models::quote::*;

#[derive(sqlx::FromRow)]
struct OwesPaid {
    total_owed: Amount,
    total_paid: Amount,
}

impl Db {
    pub async fn customer_quote_owed(&self, customer_id: i64) -> Result<Amount, AppError> {
        let owed: Amount = sqlx::query_scalar(
            "SELECT COALESCE(SUM(total_amount), 0) FROM quotes WHERE customer_id = ? AND status IN ('accepted', 'booked')",
        )
        .bind(customer_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(owed)
    }

    pub async fn customer_sale_owed(&self, customer_id: i64) -> Result<Amount, AppError> {
        let owed: Amount = sqlx::query_scalar(
            "SELECT COALESCE(SUM(total_amount), 0) FROM sales WHERE customer_id = ?",
        )
        .bind(customer_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(owed)
    }

    pub async fn customer_quote_paid(&self, customer_id: i64) -> Result<Amount, AppError> {
        let paid: Amount = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount), 0) FROM payment_utxos WHERE quote_id IN (SELECT id FROM quotes WHERE customer_id = ?)",
        )
        .bind(customer_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(paid)
    }

    pub async fn customer_sale_paid(&self, customer_id: i64) -> Result<Amount, AppError> {
        let paid: Amount = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount), 0) FROM sale_payments WHERE sale_id IN (SELECT id FROM sales WHERE customer_id = ?)",
        )
        .bind(customer_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(paid)
    }

    pub async fn total_quote_receivables(&self) -> Result<(Amount, Amount), AppError> {
        let row = sqlx::query_as::<_, OwesPaid>(
            "SELECT
                COALESCE(SUM(q.total_amount), 0) as total_owed,
                COALESCE(SUM(COALESCE(p.paid, 0)), 0) as total_paid
             FROM quotes q
             LEFT JOIN (
                SELECT quote_id, SUM(amount) as paid
                FROM payment_utxos
                GROUP BY quote_id
             ) p ON p.quote_id = q.id
             WHERE q.status IN ('accepted', 'booked')",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok((row.total_owed, row.total_paid))
    }

    pub async fn total_sale_receivables(&self) -> Result<(Amount, Amount), AppError> {
        let row = sqlx::query_as::<_, OwesPaid>(
            "SELECT
                COALESCE(SUM(s.total_amount), 0) as total_owed,
                COALESCE(SUM(COALESCE(p.paid, 0)), 0) as total_paid
             FROM sales s
             LEFT JOIN (
                SELECT sale_id, SUM(amount) as paid
                FROM sale_payments
                GROUP BY sale_id
             ) p ON p.sale_id = s.id",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok((row.total_owed, row.total_paid))
    }
}

impl Tx {
    /// Record a payment on a quote. Inserts SQL metadata and commits a ledger
    /// transaction to settle debt and record cash.
    pub async fn record_quote_payment(
        &mut self,
        quote_id: i64,
        body: &CreatePayment,
    ) -> Result<PaymentUtxo, AppError> {
        let payment = sqlx::query_as::<_, PaymentUtxo>(
            "INSERT INTO payment_utxos (quote_id, amount, method, notes) VALUES (?, ?, ?, ?) RETURNING *",
        )
        .bind(quote_id)
        .bind(body.amount)
        .bind(&body.method)
        .bind(&body.notes)
        .fetch_one(&mut *self.inner)
        .await?;
        Ok(payment)
    }

    /// Settle customer debt in the ledger and issue cash.
    pub async fn settle_customer_debt(
        &mut self,
        customer_id: i64,
        amount_cents: i64,
        idempotency_key: &str,
    ) -> Result<(), AppError> {
        let gs = self
            .ledger
            .asset("gs")
            .ok_or_else(|| AppError::Internal("gs asset not registered".into()))?;
        let gs_amount = gs.try_amount(amount_cents as i128);

        let ledger_tx = self
            .ledger
            .transaction(idempotency_key)
            .settle_debt(&customer_id.to_string(), &self.store_id, &gs_amount)
            .await
            .map_err(|e| AppError::Internal(format!("settle debt: {e}")))?
            .issue("warehouse/cash", &gs_amount)
            .map_err(|e| AppError::Internal(format!("issue cash: {e}")))?
            .build()
            .await
            .map_err(|e| AppError::Internal(format!("ledger build: {e}")))?;
        self.ledger
            .commit(ledger_tx)
            .await
            .map_err(|e| AppError::Internal(format!("ledger commit: {e}")))?;
        Ok(())
    }
}
