use crate::error::AppError;
use crate::models::booking::*;
use crate::models::quote::Quote;
use crate::version;
use super::{Db, Tx};

impl Db {
    pub async fn list_bookings(&self) -> Result<Vec<Booking>, AppError> {
        let bookings =
            sqlx::query_as::<_, Booking>("SELECT * FROM bookings ORDER BY start_at DESC")
                .fetch_all(&self.pool)
                .await?;
        Ok(bookings)
    }

    pub async fn get_booking(&self, id: i64) -> Result<Booking, AppError> {
        sqlx::query_as::<_, Booking>("SELECT * FROM bookings WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Booking not found".into()))
    }

    pub async fn get_booking_quotes(&self, booking_id: i64) -> Result<Vec<Quote>, AppError> {
        let quotes = sqlx::query_as::<_, Quote>(
            "SELECT q.* FROM quotes q
             JOIN booking_quotes bq ON bq.quote_id = q.id
             WHERE bq.booking_id = ?",
        )
        .bind(booking_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(quotes)
    }

    pub async fn calendar(
        &self,
        team_id: Option<i64>,
        start: &str,
        end: &str,
    ) -> Result<Vec<Booking>, AppError> {
        let bookings = if let Some(team_id) = team_id {
            sqlx::query_as::<_, Booking>(
                "SELECT * FROM bookings
                 WHERE team_id = ? AND start_at >= ? AND start_at <= ?
                 ORDER BY start_at ASC",
            )
            .bind(team_id)
            .bind(start)
            .bind(end)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, Booking>(
                "SELECT * FROM bookings
                 WHERE start_at >= ? AND start_at <= ?
                 ORDER BY start_at ASC",
            )
            .bind(start)
            .bind(end)
            .fetch_all(&self.pool)
            .await?
        };
        Ok(bookings)
    }
}

impl Tx {
    pub async fn create_booking(&mut self, body: &CreateBooking) -> Result<Booking, AppError> {
        let prev_booking = version::latest_version_id(&mut *self.inner, "bookings").await?;
        let booking_vid = version::compute_version_id(
            &version::booking_fields(
                body.team_id,
                body.customer_id,
                &body.title,
                &body.start_at,
                &body.end_at,
                &body.notes,
                &body.description,
                &body.location,
            ),
            &prev_booking,
        );

        let booking = sqlx::query_as::<_, Booking>(
            "INSERT INTO bookings (team_id, customer_id, title, start_at, end_at, notes, description, location, version_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING *",
        )
        .bind(body.team_id)
        .bind(body.customer_id)
        .bind(&body.title)
        .bind(&body.start_at)
        .bind(&body.end_at)
        .bind(&body.notes)
        .bind(&body.description)
        .bind(&body.location)
        .bind(&booking_vid)
        .fetch_one(&mut *self.inner)
        .await?;

        if let Some(quote_ids) = &body.quote_ids {
            for qid in quote_ids {
                sqlx::query("INSERT INTO booking_quotes (booking_id, quote_id) VALUES (?, ?)")
                    .bind(booking.id)
                    .bind(qid)
                    .execute(&mut *self.inner)
                    .await?;
            }
        }

        Ok(booking)
    }

    pub async fn update_booking(
        &mut self,
        id: i64,
        body: &serde_json::Value,
        existing: &Booking,
    ) -> Result<Booking, AppError> {
        let team_id = body["team_id"].as_i64().unwrap_or(existing.team_id);
        let booking = sqlx::query_as::<_, Booking>(
            "UPDATE bookings SET
                title = ?, start_at = ?, end_at = ?, status = ?, notes = ?, description = ?, location = ?, team_id = ?, updated_at = datetime('now')
             WHERE id = ? RETURNING *",
        )
        .bind(body["title"].as_str().unwrap_or(&existing.title))
        .bind(body["start_at"].as_str().unwrap_or(&existing.start_at))
        .bind(body["end_at"].as_str().unwrap_or(&existing.end_at))
        .bind(body["status"].as_str().unwrap_or(&existing.status))
        .bind(body["notes"].as_str().or(existing.notes.as_deref()))
        .bind(body["description"].as_str().or(existing.description.as_deref()))
        .bind(body["location"].as_str().or(existing.location.as_deref()))
        .bind(team_id)
        .bind(id)
        .fetch_one(&mut *self.inner)
        .await?;
        Ok(booking)
    }

    pub async fn link_quote(&mut self, booking_id: i64, quote_id: i64) -> Result<(), AppError> {
        sqlx::query("INSERT OR IGNORE INTO booking_quotes (booking_id, quote_id) VALUES (?, ?)")
            .bind(booking_id)
            .bind(quote_id)
            .execute(&mut *self.inner)
            .await?;
        Ok(())
    }

    pub async fn unlink_quote(&mut self, booking_id: i64, quote_id: i64) -> Result<(), AppError> {
        sqlx::query("DELETE FROM booking_quotes WHERE booking_id = ? AND quote_id = ?")
            .bind(booking_id)
            .bind(quote_id)
            .execute(&mut *self.inner)
            .await?;
        Ok(())
    }
}
