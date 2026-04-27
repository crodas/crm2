use super::{Db, Tx};
use crate::amount::Amount;
use crate::error::AppError;
use crate::models::sale::*;

impl Db {
    pub async fn list_sales(&self) -> Result<Vec<Sale>, AppError> {
        let sales = sqlx::query_as::<_, Sale>("SELECT * FROM sales ORDER BY sold_at DESC")
            .fetch_all(&self.pool)
            .await?;
        Ok(sales)
    }

    pub async fn get_sale(&self, id: i64) -> Result<Sale, AppError> {
        sqlx::query_as::<_, Sale>("SELECT * FROM sales WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Sale not found".into()))
    }

    pub async fn get_sale_lines(&self, sale_id: i64) -> Result<Vec<SaleLine>, AppError> {
        let lines = sqlx::query_as::<_, SaleLine>("SELECT * FROM sale_lines WHERE sale_id = ?")
            .bind(sale_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(lines)
    }

    pub async fn get_sale_payments(&self, sale_id: i64) -> Result<Vec<SalePayment>, AppError> {
        let payments =
            sqlx::query_as::<_, SalePayment>("SELECT * FROM sale_payments WHERE sale_id = ?")
                .bind(sale_id)
                .fetch_all(&self.pool)
                .await?;
        Ok(payments)
    }
}

impl Tx {
    /// Create a sale with lines, ledger entries, and optional immediate payment.
    /// Each tuple in `lines` is `(product_id, warehouse_id, quantity, price_per_unit_cents)`.
    pub async fn create_sale(
        &mut self,
        customer_id: i64,
        customer_group_id: i64,
        notes: Option<&str>,
        lines: &[(i64, i64, f64, i64)],
        payment_method: Option<&str>,
    ) -> Result<Sale, AppError> {
        let total: Amount = lines
            .iter()
            .map(|&(_, _, qty, price)| Amount(price).mul_qty(qty))
            .sum();

        let payment_status = if payment_method.is_some() {
            "paid"
        } else {
            "credit"
        };

        let sale = sqlx::query_as::<_, Sale>(
            "INSERT INTO sales (customer_id, customer_group_id, notes, total_amount, payment_status)
             VALUES (?, ?, ?, ?, ?) RETURNING *",
        )
        .bind(customer_id)
        .bind(customer_group_id)
        .bind(notes)
        .bind(total)
        .bind(payment_status)
        .fetch_one(&mut *self.inner)
        .await?;

        for &(product_id, _warehouse_id, quantity, price_cents) in lines {
            if quantity <= 0.0 {
                return Err(AppError::BadRequest("Quantity must be positive".into()));
            }
            sqlx::query(
                "INSERT INTO sale_lines (sale_id, product_id, quantity, price_per_unit)
                 VALUES (?, ?, ?, ?)",
            )
            .bind(sale.id)
            .bind(product_id)
            .bind(quantity)
            .bind(price_cents)
            .execute(&mut *self.inner)
            .await?;
        }

        // Ledger: debit inventory, credit customer
        let mut builder = self.ledger.transaction(format!("sale-{}", sale.id));

        for &(product_id, warehouse_id, quantity, _price_cents) in lines {
            let account = format!("warehouse/{warehouse_id}");
            let asset = self
                .ledger
                .asset(&format!("product:{product_id}"))
                .ok_or_else(|| {
                    AppError::Internal(format!("asset product:{product_id} not registered"))
                })?;
            let amount = asset
                .parse_amount(&format!("{quantity:.3}"))
                .map_err(|e| AppError::Internal(format!("parse amount: {e}")))?;

            builder = builder
                .debit(&account, &amount)
                .credit(&format!("customer/{customer_id}"), &amount);
        }

        let gs = self
            .ledger
            .asset("gs")
            .ok_or_else(|| AppError::Internal("gs asset not registered".into()))?;

        if payment_method.is_none() {
            let debt_amount = gs.try_amount(total.cents().into());
            builder = builder
                .create_debt(&customer_id.to_string(), &self.store_id, &debt_amount)
                .map_err(|e| AppError::Internal(format!("create debt: {e}")))?;
        }

        let ledger_tx = builder.build().await.map_err(|e| match e {
            ledger::Error::InsufficientBalance {
                account,
                asset: _,
                required,
                available,
            } => {
                let product_id = account
                    .split('/')
                    .last()
                    .and_then(|s| s.parse::<i64>().ok())
                    .unwrap_or(0);
                let asset_obj = self.ledger.asset(&format!("product:{product_id}"));
                let divisor = asset_obj
                    .map(|a| 10_f64.powi(a.precision() as i32))
                    .unwrap_or(1000.0);
                AppError::InsufficientStock {
                    product_id,
                    requested: required as f64 / divisor,
                    available: available as f64 / divisor,
                }
            }
            other => AppError::Internal(format!("ledger build: {other}")),
        })?;

        self.ledger
            .commit(ledger_tx)
            .await
            .map_err(|e| AppError::Internal(format!("ledger commit: {e}")))?;

        // If paid immediately: credit cash + record payment
        if let Some(method) = payment_method {
            let cash_amount = gs.try_amount(total.cents().into());
            let cash_tx = self
                .ledger
                .transaction(format!("sale-{}-cash", sale.id))
                .issue("warehouse/cash", &cash_amount)
                .map_err(|e| AppError::Internal(format!("issue: {e}")))?
                .build()
                .await
                .map_err(|e| AppError::Internal(format!("cash ledger build: {e}")))?;
            self.ledger
                .commit(cash_tx)
                .await
                .map_err(|e| AppError::Internal(format!("cash ledger commit: {e}")))?;

            sqlx::query("INSERT INTO sale_payments (sale_id, amount, method) VALUES (?, ?, ?)")
                .bind(sale.id)
                .bind(total)
                .bind(method)
                .execute(&mut *self.inner)
                .await?;
        }

        Ok(sale)
    }

    /// Record a payment on a sale. Updates payment_status if fully paid.
    /// Also settles debt in ledger.
    pub async fn record_sale_payment(
        &mut self,
        sale: &Sale,
        body: &CreateSalePayment,
    ) -> Result<SalePayment, AppError> {
        let payment = sqlx::query_as::<_, SalePayment>(
            "INSERT INTO sale_payments (sale_id, amount, method, notes) VALUES (?, ?, ?, ?) RETURNING *",
        )
        .bind(sale.id)
        .bind(body.amount)
        .bind(&body.method)
        .bind(&body.notes)
        .fetch_one(&mut *self.inner)
        .await?;

        // Settle debt in ledger
        let customer_id = sale.customer_id;
        let amount: i128 = body.amount.cents().into();

        let gs = self
            .ledger
            .asset("gs")
            .ok_or_else(|| AppError::Internal("gs asset not registered".into()))?;
        let gs_amount = gs.try_amount(amount);

        let ledger_tx = self
            .ledger
            .transaction(format!("sale-payment-{}", payment.id))
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

        // Check if fully paid
        let total_paid: Amount = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount), 0) FROM sale_payments WHERE sale_id = ?",
        )
        .bind(sale.id)
        .fetch_one(&mut *self.inner)
        .await?;

        if total_paid.cents() >= sale.total_amount.cents() {
            sqlx::query("UPDATE sales SET payment_status = 'paid' WHERE id = ?")
                .bind(sale.id)
                .execute(&mut *self.inner)
                .await?;
        }

        Ok(payment)
    }
}
