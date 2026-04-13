use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

/// Compute version_id = hex(sha256(field1|field2|...|prev_version_id))
///
/// Fields are the readonly business values of the row. Null values
/// are represented as the literal string "null". The previous
/// version_id links this row to the prior entry in the same table,
/// forming a hash chain.
pub fn compute_version_id(fields: &[String], prev_version_id: &str) -> String {
    let mut hasher = Sha256::new();
    for field in fields {
        hasher.update(field.as_bytes());
        hasher.update(b"|");
    }
    hasher.update(prev_version_id.as_bytes());
    let result = hasher.finalize();
    result.iter().map(|b| format!("{b:02x}")).collect()
}

/// Fetch the version_id of the most recent row in a table (by id DESC).
/// Returns "" if the table is empty.
pub async fn latest_version_id(
    pool: impl sqlx::SqliteExecutor<'_>,
    table: &str,
) -> Result<String, sqlx::Error> {
    // Table names are hardcoded constants from our own code, not user input.
    let sql = format!("SELECT version_id FROM {table} ORDER BY id DESC LIMIT 1");
    let result: Option<String> = sqlx::query_scalar(&sql)
        .fetch_optional(pool)
        .await?;
    Ok(result.unwrap_or_default())
}

// ── Per-table field extractors ──────────────────────────────────────

fn opt(v: &Option<impl ToString>) -> String {
    match v {
        Some(val) => val.to_string(),
        None => "null".into(),
    }
}

pub fn inventory_receipt_fields(
    reference: &Option<String>,
    supplier_name: &Option<String>,
    notes: &Option<String>,
    total_cost: i64,
) -> Vec<String> {
    vec![opt(reference), opt(supplier_name), opt(notes), total_cost.to_string()]
}

pub fn supplier_ledger_utxo_fields(
    receipt_id: i64,
    amount: i64,
    method: &Option<String>,
    notes: &Option<String>,
) -> Vec<String> {
    vec![
        receipt_id.to_string(),
        amount.to_string(),
        opt(method),
        opt(notes),
    ]
}

pub fn inventory_utxo_fields(
    product_id: i64,
    warehouse_id: i64,
    quantity: f64,
    cost_per_unit: i64,
    receipt_id: Option<i64>,
    source_sale_id: Option<i64>,
) -> Vec<String> {
    vec![
        product_id.to_string(),
        warehouse_id.to_string(),
        quantity.to_string(),
        cost_per_unit.to_string(),
        opt(&receipt_id),
        opt(&source_sale_id),
    ]
}

pub fn receipt_price_fields(
    receipt_id: i64,
    product_id: i64,
    customer_group_id: i64,
    price_per_unit: i64,
) -> Vec<String> {
    vec![
        receipt_id.to_string(),
        product_id.to_string(),
        customer_group_id.to_string(),
        price_per_unit.to_string(),
    ]
}

pub fn sale_fields(
    customer_id: i64,
    customer_group_id: i64,
    notes: &Option<String>,
    total_amount: i64,
) -> Vec<String> {
    vec![
        customer_id.to_string(),
        customer_group_id.to_string(),
        opt(notes),
        total_amount.to_string(),
    ]
}

pub fn sale_line_fields(
    sale_id: i64,
    product_id: i64,
    quantity: f64,
    price_per_unit: i64,
) -> Vec<String> {
    vec![
        sale_id.to_string(),
        product_id.to_string(),
        quantity.to_string(),
        price_per_unit.to_string(),
    ]
}

pub fn sale_line_utxo_input_fields(
    sale_line_id: i64,
    utxo_id: i64,
    quantity_used: f64,
) -> Vec<String> {
    vec![
        sale_line_id.to_string(),
        utxo_id.to_string(),
        quantity_used.to_string(),
    ]
}

pub fn payment_utxo_fields(
    quote_id: i64,
    amount: i64,
    method: &Option<String>,
    notes: &Option<String>,
) -> Vec<String> {
    vec![
        quote_id.to_string(),
        amount.to_string(),
        opt(method),
        opt(notes),
    ]
}

pub fn quote_fields(
    customer_id: i64,
    title: &str,
    description: &Option<String>,
    total_amount: i64,
    is_debt: bool,
    valid_until: &Option<String>,
) -> Vec<String> {
    vec![
        customer_id.to_string(),
        title.to_string(),
        opt(description),
        total_amount.to_string(),
        (is_debt as i64).to_string(),
        opt(valid_until),
    ]
}

pub fn quote_line_fields(
    quote_id: i64,
    description: &str,
    quantity: f64,
    unit_price: i64,
    service_id: Option<i64>,
    line_type: &str,
) -> Vec<String> {
    vec![
        quote_id.to_string(),
        description.to_string(),
        quantity.to_string(),
        unit_price.to_string(),
        opt(&service_id),
        line_type.to_string(),
    ]
}

pub fn booking_fields(
    team_id: i64,
    customer_id: i64,
    title: &str,
    start_at: &str,
    end_at: &str,
    notes: &Option<String>,
    description: &Option<String>,
    location: &Option<String>,
) -> Vec<String> {
    vec![
        team_id.to_string(),
        customer_id.to_string(),
        title.to_string(),
        start_at.to_string(),
        end_at.to_string(),
        opt(notes),
        opt(description),
        opt(location),
    ]
}

// ── Backfill for existing data after migration ──────────────────────

/// Recompute version chains for the original append-only tables (migration 006).
pub async fn recompute_append_only_chains(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    recompute_inventory_receipts(pool).await?;
    recompute_inventory_utxos(pool).await?;
    recompute_receipt_prices(pool).await?;
    recompute_sales(pool).await?;
    recompute_sale_lines(pool).await?;
    recompute_sale_line_utxo_inputs(pool).await?;
    recompute_payment_utxos(pool).await?;
    recompute_supplier_ledger_utxos(pool).await?;
    Ok(())
}

/// Recompute version chains for all versioned tables (after migration 008).
pub async fn recompute_all_chains(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    recompute_append_only_chains(pool).await?;
    recompute_quotes(pool).await?;
    recompute_quote_lines(pool).await?;
    recompute_bookings(pool).await?;
    Ok(())
}

macro_rules! recompute_chain {
    ($fn_name:ident, $table:expr, $query:expr, $row_type:ty, $field_fn:expr) => {
        async fn $fn_name(pool: &SqlitePool) -> Result<(), sqlx::Error> {
            let rows: Vec<$row_type> =
                sqlx::query_as($query).fetch_all(pool).await?;
            let mut prev = String::new();
            for row in &rows {
                let fields = $field_fn(row);
                let vid = compute_version_id(&fields, &prev);
                let sql = format!("UPDATE {} SET version_id = ? WHERE id = ?", $table);
                sqlx::query(&sql).bind(&vid).bind(row.id).execute(pool).await?;
                prev = vid;
            }
            Ok(())
        }
    };
}

// Minimal row types for recompute (avoid coupling to model structs)
#[derive(sqlx::FromRow)]
struct RcptRow { id: i64, reference: Option<String>, supplier_name: Option<String>, notes: Option<String>, total_cost: i64 }
#[derive(sqlx::FromRow)]
struct SupplierLedgerRow { id: i64, receipt_id: i64, amount: i64, method: Option<String>, notes: Option<String> }
#[derive(sqlx::FromRow)]
struct UtxoRow { id: i64, product_id: i64, warehouse_id: i64, quantity: f64, cost_per_unit: i64, receipt_id: Option<i64>, source_sale_id: Option<i64> }
#[derive(sqlx::FromRow)]
struct RcptPriceRow { id: i64, receipt_id: i64, product_id: i64, customer_group_id: i64, price_per_unit: i64 }
#[derive(sqlx::FromRow)]
struct SaleRow { id: i64, customer_id: i64, customer_group_id: i64, notes: Option<String>, total_amount: i64 }
#[derive(sqlx::FromRow)]
struct SaleLineRow { id: i64, sale_id: i64, product_id: i64, quantity: f64, price_per_unit: i64 }
#[derive(sqlx::FromRow)]
struct SaleLineUtxoRow { id: i64, sale_line_id: i64, utxo_id: i64, quantity_used: f64 }
#[derive(sqlx::FromRow)]
struct PayRow { id: i64, quote_id: i64, amount: i64, method: Option<String>, notes: Option<String> }
#[derive(sqlx::FromRow)]
struct QuoteRow { id: i64, customer_id: i64, title: String, description: Option<String>, total_amount: i64, is_debt: bool, valid_until: Option<String> }
#[derive(sqlx::FromRow)]
struct QuoteLineRow { id: i64, quote_id: i64, description: String, quantity: f64, unit_price: i64, service_id: Option<i64>, line_type: String }
#[derive(sqlx::FromRow)]
struct BookingRow { id: i64, team_id: i64, customer_id: i64, title: String, start_at: String, end_at: String, notes: Option<String>, description: Option<String>, location: Option<String> }

recompute_chain!(recompute_inventory_receipts, "inventory_receipts",
    "SELECT id, reference, supplier_name, notes, total_cost FROM inventory_receipts ORDER BY id ASC",
    RcptRow, |r: &RcptRow| inventory_receipt_fields(&r.reference, &r.supplier_name, &r.notes, r.total_cost));

recompute_chain!(recompute_supplier_ledger_utxos, "supplier_ledger_utxos",
    "SELECT id, receipt_id, amount, method, notes FROM supplier_ledger_utxos ORDER BY id ASC",
    SupplierLedgerRow, |r: &SupplierLedgerRow| supplier_ledger_utxo_fields(r.receipt_id, r.amount, &r.method, &r.notes));

recompute_chain!(recompute_inventory_utxos, "inventory_utxos",
    "SELECT id, product_id, warehouse_id, quantity, cost_per_unit, receipt_id, source_sale_id FROM inventory_utxos ORDER BY id ASC",
    UtxoRow, |r: &UtxoRow| inventory_utxo_fields(r.product_id, r.warehouse_id, r.quantity, r.cost_per_unit, r.receipt_id, r.source_sale_id));

recompute_chain!(recompute_receipt_prices, "inventory_receipt_prices",
    "SELECT id, receipt_id, product_id, customer_group_id, price_per_unit FROM inventory_receipt_prices ORDER BY id ASC",
    RcptPriceRow, |r: &RcptPriceRow| receipt_price_fields(r.receipt_id, r.product_id, r.customer_group_id, r.price_per_unit));

recompute_chain!(recompute_sales, "sales",
    "SELECT id, customer_id, customer_group_id, notes, total_amount FROM sales ORDER BY id ASC",
    SaleRow, |r: &SaleRow| sale_fields(r.customer_id, r.customer_group_id, &r.notes, r.total_amount));

recompute_chain!(recompute_sale_lines, "sale_lines",
    "SELECT id, sale_id, product_id, quantity, price_per_unit FROM sale_lines ORDER BY id ASC",
    SaleLineRow, |r: &SaleLineRow| sale_line_fields(r.sale_id, r.product_id, r.quantity, r.price_per_unit));

recompute_chain!(recompute_sale_line_utxo_inputs, "sale_line_utxo_inputs",
    "SELECT id, sale_line_id, utxo_id, quantity_used FROM sale_line_utxo_inputs ORDER BY id ASC",
    SaleLineUtxoRow, |r: &SaleLineUtxoRow| sale_line_utxo_input_fields(r.sale_line_id, r.utxo_id, r.quantity_used));

recompute_chain!(recompute_payment_utxos, "payment_utxos",
    "SELECT id, quote_id, amount, method, notes FROM payment_utxos ORDER BY id ASC",
    PayRow, |r: &PayRow| payment_utxo_fields(r.quote_id, r.amount, &r.method, &r.notes));

recompute_chain!(recompute_quotes, "quotes",
    "SELECT id, customer_id, title, description, total_amount, is_debt, valid_until FROM quotes ORDER BY id ASC",
    QuoteRow, |r: &QuoteRow| quote_fields(r.customer_id, &r.title, &r.description, r.total_amount, r.is_debt, &r.valid_until));

recompute_chain!(recompute_quote_lines, "quote_lines",
    "SELECT id, quote_id, description, quantity, unit_price, service_id, line_type FROM quote_lines ORDER BY id ASC",
    QuoteLineRow, |r: &QuoteLineRow| quote_line_fields(r.quote_id, &r.description, r.quantity, r.unit_price, r.service_id, &r.line_type));

recompute_chain!(recompute_bookings, "bookings",
    "SELECT id, team_id, customer_id, title, start_at, end_at, notes, description, location FROM bookings ORDER BY id ASC",
    BookingRow, |r: &BookingRow| booking_fields(r.team_id, r.customer_id, &r.title, &r.start_at, &r.end_at, &r.notes, &r.description, &r.location));
