use crate::error::AppError;
use crate::models::customer::*;
use super::{Db, Tx};

impl Db {
    pub async fn list_customer_types(&self) -> Result<Vec<CustomerType>, AppError> {
        let types = sqlx::query_as::<_, CustomerType>(
            "SELECT * FROM customer_types ORDER BY sort_order, name",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(types)
    }

    pub async fn list_customers(
        &self,
        type_id: Option<i64>,
        search: Option<&str>,
    ) -> Result<Vec<Customer>, AppError> {
        let customers = if let Some(search) = search {
            let pattern = format!("%{search}%");
            if let Some(type_id) = type_id {
                sqlx::query_as::<_, Customer>(
                    "SELECT * FROM customers WHERE customer_type_id = ? AND (name LIKE ? OR email LIKE ? OR phone LIKE ?) ORDER BY name",
                )
                .bind(type_id)
                .bind(&pattern)
                .bind(&pattern)
                .bind(&pattern)
                .fetch_all(&self.pool)
                .await?
            } else {
                sqlx::query_as::<_, Customer>(
                    "SELECT * FROM customers WHERE name LIKE ? OR email LIKE ? OR phone LIKE ? ORDER BY name",
                )
                .bind(&pattern)
                .bind(&pattern)
                .bind(&pattern)
                .fetch_all(&self.pool)
                .await?
            }
        } else if let Some(type_id) = type_id {
            sqlx::query_as::<_, Customer>(
                "SELECT * FROM customers WHERE customer_type_id = ? ORDER BY name",
            )
            .bind(type_id)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, Customer>("SELECT * FROM customers ORDER BY name")
                .fetch_all(&self.pool)
                .await?
        };
        Ok(customers)
    }

    pub async fn get_customer(&self, id: i64) -> Result<Customer, AppError> {
        sqlx::query_as::<_, Customer>("SELECT * FROM customers WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Customer not found".into()))
    }

    pub async fn customer_timeline(&self, id: i64) -> Result<Vec<TimelineEvent>, AppError> {
        let events = sqlx::query_as::<_, TimelineEvent>(
            "SELECT 'quote' as event_type, id, title as summary, created_at as date, total_amount as amount
             FROM quotes WHERE customer_id = ?1
             UNION ALL
             SELECT 'sale' as event_type, id, COALESCE(notes, 'Sale') as summary, sold_at as date, total_amount as amount
             FROM sales WHERE customer_id = ?1
             UNION ALL
             SELECT 'booking' as event_type, id, title as summary, start_at as date, NULL as amount
             FROM bookings WHERE customer_id = ?1
             UNION ALL
             SELECT 'payment' as event_type, p.id, ('Payment on: ' || q.title) as summary, p.paid_at as date, p.amount as amount
             FROM payment_utxos p
             JOIN quotes q ON q.id = p.quote_id
             WHERE q.customer_id = ?1
             ORDER BY date DESC",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        Ok(events)
    }

    pub async fn list_customer_groups(&self) -> Result<Vec<CustomerGroup>, AppError> {
        let groups =
            sqlx::query_as::<_, CustomerGroup>("SELECT * FROM customer_groups ORDER BY id")
                .fetch_all(&self.pool)
                .await?;
        Ok(groups)
    }

    pub async fn get_customer_type_name(&self, id: i64) -> Result<Option<String>, AppError> {
        let name: Option<String> =
            sqlx::query_scalar("SELECT name FROM customer_types WHERE id = ?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(name)
    }
}

impl Tx {
    pub async fn reorder_customer_types(&mut self, ids: &[i64]) -> Result<(), AppError> {
        for (i, id) in ids.iter().enumerate() {
            sqlx::query("UPDATE customer_types SET sort_order = ? WHERE id = ?")
                .bind(i as i64)
                .bind(id)
                .execute(&mut *self.inner)
                .await?;
        }
        Ok(())
    }

    pub async fn create_customer_type(&mut self, name: &str) -> Result<CustomerType, AppError> {
        let ct = sqlx::query_as::<_, CustomerType>(
            "INSERT INTO customer_types (name) VALUES (?) RETURNING *",
        )
        .bind(name)
        .fetch_one(&mut *self.inner)
        .await?;
        Ok(ct)
    }

    pub async fn update_customer_type(
        &mut self,
        id: i64,
        name: &str,
    ) -> Result<CustomerType, AppError> {
        sqlx::query_as::<_, CustomerType>(
            "UPDATE customer_types SET name = ? WHERE id = ? RETURNING *",
        )
        .bind(name)
        .bind(id)
        .fetch_optional(&mut *self.inner)
        .await?
        .ok_or_else(|| AppError::NotFound("Customer type not found".into()))
    }

    pub async fn create_customer(&mut self, body: &CreateCustomer) -> Result<Customer, AppError> {
        let customer = sqlx::query_as::<_, Customer>(
            "INSERT INTO customers (customer_type_id, name, email, phone, address, notes)
             VALUES (?, ?, ?, ?, ?, ?) RETURNING *",
        )
        .bind(body.customer_type_id)
        .bind(&body.name)
        .bind(&body.email)
        .bind(&body.phone)
        .bind(&body.address)
        .bind(&body.notes)
        .fetch_one(&mut *self.inner)
        .await?;
        Ok(customer)
    }

    pub async fn update_customer(
        &mut self,
        id: i64,
        body: &UpdateCustomer,
        existing: &Customer,
    ) -> Result<Customer, AppError> {
        let customer = sqlx::query_as::<_, Customer>(
            "UPDATE customers SET
                customer_type_id = ?, name = ?, email = ?, phone = ?, address = ?, notes = ?,
                updated_at = datetime('now')
             WHERE id = ? RETURNING *",
        )
        .bind(body.customer_type_id.unwrap_or(existing.customer_type_id))
        .bind(body.name.as_deref().unwrap_or(&existing.name))
        .bind(body.email.as_deref().or(existing.email.as_deref()))
        .bind(body.phone.as_deref().or(existing.phone.as_deref()))
        .bind(body.address.as_deref().or(existing.address.as_deref()))
        .bind(body.notes.as_deref().or(existing.notes.as_deref()))
        .bind(id)
        .fetch_one(&mut *self.inner)
        .await?;
        Ok(customer)
    }

    pub async fn create_customer_group(
        &mut self,
        name: &str,
        type_id: i64,
        markup: f64,
    ) -> Result<CustomerGroup, AppError> {
        let group = sqlx::query_as::<_, CustomerGroup>(
            "INSERT INTO customer_groups (name, customer_type_id, default_markup_pct) VALUES (?, ?, ?) RETURNING *",
        )
        .bind(name)
        .bind(type_id)
        .bind(markup)
        .fetch_one(&mut *self.inner)
        .await?;
        Ok(group)
    }

    pub async fn update_customer_group(
        &mut self,
        id: i64,
        markup: f64,
    ) -> Result<CustomerGroup, AppError> {
        sqlx::query_as::<_, CustomerGroup>(
            "UPDATE customer_groups SET default_markup_pct = ? WHERE id = ? RETURNING *",
        )
        .bind(markup)
        .bind(id)
        .fetch_one(&mut *self.inner)
        .await
        .map_err(Into::into)
    }

    pub async fn get_customer_group(
        &mut self,
        id: i64,
    ) -> Result<CustomerGroup, AppError> {
        sqlx::query_as::<_, CustomerGroup>("SELECT * FROM customer_groups WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *self.inner)
            .await?
            .ok_or_else(|| AppError::NotFound("Customer group not found".into()))
    }
}
