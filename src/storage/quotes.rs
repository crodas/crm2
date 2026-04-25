use crate::amount::Amount;
use crate::error::AppError;
use crate::models::booking::Booking;
use crate::models::quote::*;
use crate::version;
use super::{Db, Tx};

impl Db {
    pub async fn list_quotes(
        &self,
        customer_id: Option<i64>,
        status: Option<&str>,
    ) -> Result<Vec<Quote>, AppError> {
        let quotes = if let Some(cid) = customer_id {
            if let Some(status) = status {
                sqlx::query_as::<_, Quote>(
                    "SELECT * FROM quotes WHERE customer_id = ? AND status = ? ORDER BY created_at DESC",
                )
                .bind(cid)
                .bind(status)
                .fetch_all(&self.pool)
                .await?
            } else {
                sqlx::query_as::<_, Quote>(
                    "SELECT * FROM quotes WHERE customer_id = ? ORDER BY created_at DESC",
                )
                .bind(cid)
                .fetch_all(&self.pool)
                .await?
            }
        } else if let Some(status) = status {
            sqlx::query_as::<_, Quote>(
                "SELECT * FROM quotes WHERE status = ? ORDER BY created_at DESC",
            )
            .bind(status)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, Quote>("SELECT * FROM quotes ORDER BY created_at DESC")
                .fetch_all(&self.pool)
                .await?
        };
        Ok(quotes)
    }

    pub async fn get_quote(&self, id: i64) -> Result<Quote, AppError> {
        sqlx::query_as::<_, Quote>("SELECT * FROM quotes WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Quote not found".into()))
    }

    pub async fn get_quote_lines(&self, quote_id: i64) -> Result<Vec<QuoteLine>, AppError> {
        let lines =
            sqlx::query_as::<_, QuoteLine>("SELECT * FROM quote_lines WHERE quote_id = ?")
                .bind(quote_id)
                .fetch_all(&self.pool)
                .await?;
        Ok(lines)
    }

    pub async fn get_quote_payments(&self, quote_id: i64) -> Result<Vec<PaymentUtxo>, AppError> {
        let payments =
            sqlx::query_as::<_, PaymentUtxo>("SELECT * FROM payment_utxos WHERE quote_id = ?")
                .bind(quote_id)
                .fetch_all(&self.pool)
                .await?;
        Ok(payments)
    }

    pub async fn get_quote_bookings(&self, quote_id: i64) -> Result<Vec<Booking>, AppError> {
        let bookings = sqlx::query_as::<_, Booking>(
            "SELECT b.* FROM bookings b
             JOIN booking_quotes bq ON bq.booking_id = b.id
             WHERE bq.quote_id = ?",
        )
        .bind(quote_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(bookings)
    }
}

impl Tx {
    pub async fn create_quote(&mut self, body: &CreateQuote) -> Result<Quote, AppError> {
        let total: Amount = body
            .lines
            .iter()
            .map(|l| l.unit_price.mul_qty(l.quantity))
            .sum();

        let prev_quote = version::latest_version_id(&mut *self.inner, "quotes").await?;
        let quote_vid = version::compute_version_id(
            &version::quote_fields(
                body.customer_id,
                &body.title,
                &body.description,
                total.cents(),
                false,
                &body.valid_until,
            ),
            &prev_quote,
        );

        let quote = sqlx::query_as::<_, Quote>(
            "INSERT INTO quotes (customer_id, title, description, total_amount, valid_until, version_id)
             VALUES (?, ?, ?, ?, ?, ?) RETURNING *",
        )
        .bind(body.customer_id)
        .bind(&body.title)
        .bind(&body.description)
        .bind(total)
        .bind(&body.valid_until)
        .bind(&quote_vid)
        .fetch_one(&mut *self.inner)
        .await?;

        for line in &body.lines {
            let line_type = line.line_type.as_deref().unwrap_or("item");
            let prev_ql = version::latest_version_id(&mut *self.inner, "quote_lines").await?;
            let ql_vid = version::compute_version_id(
                &version::quote_line_fields(
                    quote.id,
                    &line.description,
                    line.quantity,
                    line.unit_price.cents(),
                    line.service_id,
                    line_type,
                ),
                &prev_ql,
            );

            sqlx::query(
                "INSERT INTO quote_lines (quote_id, description, quantity, unit_price, service_id, line_type, version_id)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(quote.id)
            .bind(&line.description)
            .bind(line.quantity)
            .bind(line.unit_price)
            .bind(line.service_id)
            .bind(line_type)
            .bind(&ql_vid)
            .execute(&mut *self.inner)
            .await?;
        }

        Ok(quote)
    }

    pub async fn update_quote(
        &mut self,
        id: i64,
        body: &serde_json::Value,
        existing: &Quote,
    ) -> Result<Quote, AppError> {
        let quote = sqlx::query_as::<_, Quote>(
            "UPDATE quotes SET title = ?, description = ?, valid_until = ?, updated_at = datetime('now')
             WHERE id = ? RETURNING *",
        )
        .bind(body["title"].as_str().unwrap_or(&existing.title))
        .bind(body["description"].as_str().or(existing.description.as_deref()))
        .bind(body["valid_until"].as_str().or(existing.valid_until.as_deref()))
        .bind(id)
        .fetch_one(&mut *self.inner)
        .await?;
        Ok(quote)
    }

    pub async fn update_quote_status(
        &mut self,
        id: i64,
        status: &str,
    ) -> Result<Quote, AppError> {
        sqlx::query_as::<_, Quote>(
            "UPDATE quotes SET status = ?, updated_at = datetime('now') WHERE id = ? RETURNING *",
        )
        .bind(status)
        .bind(id)
        .fetch_optional(&mut *self.inner)
        .await?
        .ok_or_else(|| AppError::NotFound("Quote not found".into()))
    }

    pub async fn create_debt(&mut self, body: &CreateDebt) -> Result<Quote, AppError> {
        let prev_quote = version::latest_version_id(&mut *self.inner, "quotes").await?;
        let quote_vid = version::compute_version_id(
            &version::quote_fields(
                body.customer_id,
                &body.title,
                &body.description,
                body.amount.cents(),
                true,
                &None,
            ),
            &prev_quote,
        );

        let quote = sqlx::query_as::<_, Quote>(
            "INSERT INTO quotes (customer_id, status, title, description, total_amount, is_debt, version_id)
             VALUES (?, 'accepted', ?, ?, ?, 1, ?) RETURNING *",
        )
        .bind(body.customer_id)
        .bind(&body.title)
        .bind(&body.description)
        .bind(body.amount)
        .bind(&quote_vid)
        .fetch_one(&mut *self.inner)
        .await?;

        let prev_ql = version::latest_version_id(&mut *self.inner, "quote_lines").await?;
        let ql_vid = version::compute_version_id(
            &version::quote_line_fields(
                quote.id,
                &body.title,
                1.0,
                body.amount.cents(),
                None,
                "item",
            ),
            &prev_ql,
        );

        sqlx::query(
            "INSERT INTO quote_lines (quote_id, description, quantity, unit_price, version_id)
             VALUES (?, ?, 1, ?, ?)",
        )
        .bind(quote.id)
        .bind(&body.title)
        .bind(body.amount)
        .bind(&ql_vid)
        .execute(&mut *self.inner)
        .await?;

        Ok(quote)
    }
}
