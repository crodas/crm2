use std::collections::HashMap;

use crate::amount::Amount;
use crate::error::AppError;
use crate::models::inventory::{LatestPrice, LatestPriceQuery};
use crate::models::product::*;
use super::{Db, Tx};

#[derive(sqlx::FromRow)]
struct PriceRow {
    product_id: i64,
    group_name: String,
    price_per_unit: Amount,
}

impl Db {
    pub async fn list_products(
        &self,
        product_type: Option<&str>,
    ) -> Result<Vec<Product>, AppError> {
        let products = if let Some(pt) = product_type {
            sqlx::query_as::<_, Product>(
                "SELECT * FROM products WHERE product_type = ? ORDER BY name",
            )
            .bind(pt)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, Product>("SELECT * FROM products ORDER BY product_type, name")
                .fetch_all(&self.pool)
                .await?
        };
        Ok(products)
    }

    pub async fn get_product(&self, id: i64) -> Result<Product, AppError> {
        sqlx::query_as::<_, Product>("SELECT * FROM products WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Product not found".into()))
    }

    pub async fn fetch_latest_prices(
        &self,
    ) -> Result<HashMap<i64, HashMap<String, Amount>>, AppError> {
        let rows = sqlx::query_as::<_, PriceRow>(
            "SELECT p.product_id, cg.name as group_name, p.price_per_unit
             FROM inventory_receipt_prices p
             INNER JOIN customer_groups cg ON cg.id = p.customer_group_id
             INNER JOIN (
                 SELECT product_id, customer_group_id, MAX(receipt_id) as max_receipt_id
                 FROM inventory_receipt_prices
                 GROUP BY product_id, customer_group_id
             ) latest ON p.product_id = latest.product_id
                      AND p.customer_group_id = latest.customer_group_id
                      AND p.receipt_id = latest.max_receipt_id",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut map: HashMap<i64, HashMap<String, Amount>> = HashMap::new();
        for row in rows {
            map.entry(row.product_id)
                .or_default()
                .insert(row.group_name, row.price_per_unit);
        }
        Ok(map)
    }

    pub async fn list_warehouses(&self) -> Result<Vec<Warehouse>, AppError> {
        let warehouses =
            sqlx::query_as::<_, Warehouse>("SELECT * FROM warehouses ORDER BY sort_order, name")
                .fetch_all(&self.pool)
                .await?;
        Ok(warehouses)
    }

    pub async fn warehouse_exists(&self, id: i64) -> Result<bool, AppError> {
        let row: Option<i64> =
            sqlx::query_scalar("SELECT id FROM warehouses WHERE id = ?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.is_some())
    }

    pub async fn latest_prices(
        &self,
        params: &LatestPriceQuery,
    ) -> Result<Vec<LatestPrice>, AppError> {
        let mut where_clauses = Vec::new();
        if params.product_id.is_some() {
            where_clauses.push("product_id = ?");
        }
        if params.customer_group_id.is_some() {
            where_clauses.push("customer_group_id = ?");
        }
        let where_sql = if where_clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_clauses.join(" AND "))
        };

        let sql = format!(
            "SELECT p.product_id, p.customer_group_id, p.price_per_unit
             FROM inventory_receipt_prices p
             INNER JOIN (
                 SELECT product_id, customer_group_id, MAX(receipt_id) as max_receipt_id
                 FROM inventory_receipt_prices
                 {where_sql}
                 GROUP BY product_id, customer_group_id
             ) latest ON p.product_id = latest.product_id
                      AND p.customer_group_id = latest.customer_group_id
                      AND p.receipt_id = latest.max_receipt_id"
        );

        let mut query = sqlx::query_as::<_, LatestPrice>(&sql);
        if let Some(pid) = params.product_id {
            query = query.bind(pid);
        }
        if let Some(gid) = params.customer_group_id {
            query = query.bind(gid);
        }

        let prices = query.fetch_all(&self.pool).await?;
        Ok(prices)
    }

    pub async fn product_ids(&self) -> Result<Vec<i64>, AppError> {
        let ids: Vec<(i64,)> = sqlx::query_as("SELECT id FROM products")
            .fetch_all(&self.pool)
            .await?;
        Ok(ids.into_iter().map(|(id,)| id).collect())
    }
}

impl Tx {
    pub async fn create_product(&mut self, body: &CreateProduct) -> Result<Product, AppError> {
        let pt = body.product_type.as_deref().unwrap_or("product");
        let sku = match &body.sku {
            Some(s) if !s.is_empty() => s.clone(),
            _ if pt == "service" => format!("SVC-{}", uuid::Uuid::new_v4()),
            _ => String::new(),
        };
        let sku_opt = if sku.is_empty() { None } else { Some(sku) };

        let product = sqlx::query_as::<_, Product>(
            "INSERT INTO products (sku, name, description, unit, product_type, suggested_price)
             VALUES (?, ?, ?, ?, ?, ?) RETURNING *",
        )
        .bind(&sku_opt)
        .bind(&body.name)
        .bind(&body.description)
        .bind(body.unit.as_deref().unwrap_or("unit"))
        .bind(pt)
        .bind(body.suggested_price.unwrap_or(Amount(0)))
        .fetch_one(&mut *self.inner)
        .await?;

        // Register a ledger asset for this product
        self.ledger
            .register_asset(ledger::Asset::new(format!("product:{}", product.id), 3))
            .await
            .map_err(|e| AppError::Internal(format!("register asset: {e}")))?;

        Ok(product)
    }

    pub async fn update_product(
        &mut self,
        id: i64,
        body: &serde_json::Value,
        existing: &Product,
    ) -> Result<Product, AppError> {
        let product = sqlx::query_as::<_, Product>(
            "UPDATE products SET sku = ?, name = ?, description = ?, unit = ?, product_type = ?, suggested_price = ?, updated_at = datetime('now')
             WHERE id = ? RETURNING *",
        )
        .bind(body["sku"].as_str().or(existing.sku.as_deref()))
        .bind(body["name"].as_str().unwrap_or(&existing.name))
        .bind(body["description"].as_str().or(existing.description.as_deref()))
        .bind(body["unit"].as_str().unwrap_or(&existing.unit))
        .bind(body["product_type"].as_str().unwrap_or(&existing.product_type))
        .bind(body["suggested_price"].as_f64().map(Amount::from_float).unwrap_or(existing.suggested_price))
        .bind(id)
        .fetch_one(&mut *self.inner)
        .await?;
        Ok(product)
    }

    pub async fn create_warehouse(&mut self, body: &CreateWarehouse) -> Result<Warehouse, AppError> {
        let max_order: Option<i64> =
            sqlx::query_scalar("SELECT MAX(sort_order) FROM warehouses")
                .fetch_one(&mut *self.inner)
                .await?;
        let next_order = max_order.unwrap_or(0) + 1;

        let warehouse = sqlx::query_as::<_, Warehouse>(
            "INSERT INTO warehouses (name, address, sort_order) VALUES (?, ?, ?) RETURNING *",
        )
        .bind(&body.name)
        .bind(&body.address)
        .bind(next_order)
        .fetch_one(&mut *self.inner)
        .await?;
        Ok(warehouse)
    }

    pub async fn update_warehouse(
        &mut self,
        id: i64,
        body: &CreateWarehouse,
    ) -> Result<Warehouse, AppError> {
        sqlx::query_as::<_, Warehouse>(
            "UPDATE warehouses SET name = ?, address = ? WHERE id = ? RETURNING *",
        )
        .bind(&body.name)
        .bind(&body.address)
        .bind(id)
        .fetch_optional(&mut *self.inner)
        .await?
        .ok_or_else(|| AppError::NotFound("Warehouse not found".into()))
    }

    pub async fn reorder_warehouses(&mut self, ids: &[i64]) -> Result<(), AppError> {
        for (i, id) in ids.iter().enumerate() {
            sqlx::query("UPDATE warehouses SET sort_order = ? WHERE id = ?")
                .bind(i as i64)
                .bind(id)
                .execute(&mut *self.inner)
                .await?;
        }
        Ok(())
    }
}
