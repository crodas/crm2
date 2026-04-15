use axum::{
    extract::{Path, State},
    Json,
};
use sqlx::SqlitePool;

use crate::error::AppError;
use crate::models::team::*;

pub async fn list_teams(State(pool): State<SqlitePool>) -> Result<Json<Vec<Team>>, AppError> {
    let teams = sqlx::query_as::<_, Team>("SELECT * FROM teams ORDER BY name")
        .fetch_all(&pool)
        .await?;
    Ok(Json(teams))
}

pub async fn create_team(
    State(pool): State<SqlitePool>,
    Json(body): Json<CreateTeam>,
) -> Result<Json<Team>, AppError> {
    let team =
        sqlx::query_as::<_, Team>("INSERT INTO teams (name, color) VALUES (?, ?) RETURNING *")
            .bind(&body.name)
            .bind(&body.color)
            .fetch_one(&pool)
            .await?;
    Ok(Json(team))
}

pub async fn update_team(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    Json(body): Json<CreateTeam>,
) -> Result<Json<Team>, AppError> {
    let team =
        sqlx::query_as::<_, Team>("UPDATE teams SET name = ?, color = ? WHERE id = ? RETURNING *")
            .bind(&body.name)
            .bind(&body.color)
            .bind(id)
            .fetch_optional(&pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Team not found".into()))?;
    Ok(Json(team))
}

pub async fn list_members(
    State(pool): State<SqlitePool>,
    Path(team_id): Path<i64>,
) -> Result<Json<Vec<TeamMember>>, AppError> {
    let members = sqlx::query_as::<_, TeamMember>(
        "SELECT * FROM team_members WHERE team_id = ? ORDER BY name",
    )
    .bind(team_id)
    .fetch_all(&pool)
    .await?;
    Ok(Json(members))
}

pub async fn add_member(
    State(pool): State<SqlitePool>,
    Path(team_id): Path<i64>,
    Json(body): Json<CreateTeamMember>,
) -> Result<Json<TeamMember>, AppError> {
    let member = sqlx::query_as::<_, TeamMember>(
        "INSERT INTO team_members (team_id, name, role) VALUES (?, ?, ?) RETURNING *",
    )
    .bind(team_id)
    .bind(&body.name)
    .bind(&body.role)
    .fetch_one(&pool)
    .await?;
    Ok(Json(member))
}
