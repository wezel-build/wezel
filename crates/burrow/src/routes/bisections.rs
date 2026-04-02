use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use sqlx::PgPool;

use crate::models::{Bisection, BisectionJson};
use crate::{ApiResult, ise};

#[derive(Deserialize)]
pub struct BisectionListQuery {
    status: Option<String>,
    branch: Option<String>,
}

pub async fn get_project_bisections(
    Path(project_id): Path<i64>,
    Query(q): Query<BisectionListQuery>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<Vec<BisectionJson>>> {
    let rows = sqlx::query_as::<_, Bisection>(
        "SELECT id, project_id, experiment_name, measurement_name, branch, \
                good_sha, bad_sha, good_value, bad_value, status, culprit_sha \
         FROM bisections \
         WHERE project_id = $1 \
           AND ($2::text IS NULL OR status = $2) \
           AND ($3::text IS NULL OR branch = $3) \
         ORDER BY id DESC",
    )
    .bind(project_id)
    .bind(q.status.as_deref())
    .bind(q.branch.as_deref())
    .fetch_all(&pool)
    .await
    .map_err(ise)?;

    Ok(Json(rows.into_iter().map(BisectionJson::from).collect()))
}

pub async fn get_project_bisection(
    Path((project_id, bisection_id)): Path<(i64, i64)>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<BisectionJson>> {
    let row = sqlx::query_as::<_, Bisection>(
        "SELECT id, project_id, experiment_name, measurement_name, branch, \
                good_sha, bad_sha, good_value, bad_value, status, culprit_sha \
         FROM bisections \
         WHERE id = $1 AND project_id = $2",
    )
    .bind(bisection_id)
    .bind(project_id)
    .fetch_optional(&pool)
    .await
    .map_err(ise)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(BisectionJson::from(row)))
}

#[derive(Deserialize)]
pub struct BisectionPatchBody {
    status: String,
}

pub async fn patch_project_bisection(
    Path((project_id, bisection_id)): Path<(i64, i64)>,
    State(pool): State<PgPool>,
    Json(body): Json<BisectionPatchBody>,
) -> ApiResult<StatusCode> {
    if body.status != "abandoned" {
        return Err(StatusCode::BAD_REQUEST);
    }

    let rows_affected = sqlx::query(
        "UPDATE bisections \
         SET status = 'abandoned', completed_at = now() \
         WHERE id = $1 AND project_id = $2 AND status = 'active'",
    )
    .bind(bisection_id)
    .bind(project_id)
    .execute(&pool)
    .await
    .map_err(ise)?
    .rows_affected();

    if rows_affected == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::OK)
}
