use sha2::{Digest, Sha256};

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
    let result: Option<String> = sqlx::query_scalar(&sql).fetch_optional(pool).await?;
    Ok(result.unwrap_or_default())
}

// ── Per-table field extractors ──────────────────────────────────────

fn opt(v: &Option<impl ToString>) -> String {
    match v {
        Some(val) => val.to_string(),
        None => "null".into(),
    }
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
