use super::{Db, Tx};
use crate::error::AppError;
use crate::models::team::*;

impl Db {
    pub async fn list_teams(&self) -> Result<Vec<Team>, AppError> {
        let teams = sqlx::query_as::<_, Team>("SELECT * FROM teams ORDER BY name")
            .fetch_all(&self.pool)
            .await?;
        Ok(teams)
    }

    pub async fn list_members(&self, team_id: i64) -> Result<Vec<TeamMember>, AppError> {
        let members = sqlx::query_as::<_, TeamMember>(
            "SELECT * FROM team_members WHERE team_id = ? ORDER BY name",
        )
        .bind(team_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(members)
    }
}

impl Tx {
    pub async fn create_team(&mut self, body: &CreateTeam) -> Result<Team, AppError> {
        let team =
            sqlx::query_as::<_, Team>("INSERT INTO teams (name, color) VALUES (?, ?) RETURNING *")
                .bind(&body.name)
                .bind(&body.color)
                .fetch_one(&mut *self.inner)
                .await?;
        Ok(team)
    }

    pub async fn update_team(&mut self, id: i64, body: &CreateTeam) -> Result<Team, AppError> {
        let team = sqlx::query_as::<_, Team>(
            "UPDATE teams SET name = ?, color = ? WHERE id = ? RETURNING *",
        )
        .bind(&body.name)
        .bind(&body.color)
        .bind(id)
        .fetch_optional(&mut *self.inner)
        .await?
        .ok_or_else(|| AppError::NotFound("Team not found".into()))?;
        Ok(team)
    }

    pub async fn add_member(
        &mut self,
        team_id: i64,
        body: &CreateTeamMember,
    ) -> Result<TeamMember, AppError> {
        let member = sqlx::query_as::<_, TeamMember>(
            "INSERT INTO team_members (team_id, name, role) VALUES (?, ?, ?) RETURNING *",
        )
        .bind(team_id)
        .bind(&body.name)
        .bind(&body.role)
        .fetch_one(&mut *self.inner)
        .await?;
        Ok(member)
    }
}
