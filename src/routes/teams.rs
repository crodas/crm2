use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

use crate::error::AppError;
use crate::models::team::*;
use crate::state::AppState;

pub async fn list_teams(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Team>>, AppError> {
    let teams = state.db.list_teams().await?;
    Ok(Json(teams))
}

pub async fn create_team(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateTeam>,
) -> Result<Json<Team>, AppError> {
    let mut tx = state.db.begin().await?;
    let team = tx.create_team(&body).await?;
    tx.commit().await?;
    Ok(Json(team))
}

pub async fn update_team(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(body): Json<CreateTeam>,
) -> Result<Json<Team>, AppError> {
    let mut tx = state.db.begin().await?;
    let team = tx.update_team(id, &body).await?;
    tx.commit().await?;
    Ok(Json(team))
}

pub async fn list_members(
    State(state): State<Arc<AppState>>,
    Path(team_id): Path<i64>,
) -> Result<Json<Vec<TeamMember>>, AppError> {
    let members = state.db.list_members(team_id).await?;
    Ok(Json(members))
}

pub async fn add_member(
    State(state): State<Arc<AppState>>,
    Path(team_id): Path<i64>,
    Json(body): Json<CreateTeamMember>,
) -> Result<Json<TeamMember>, AppError> {
    let mut tx = state.db.begin().await?;
    let member = tx.add_member(team_id, &body).await?;
    tx.commit().await?;
    Ok(Json(member))
}
