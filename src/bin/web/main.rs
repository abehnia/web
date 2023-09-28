#![warn(clippy::pedantic)]

use std::{net::SocketAddr, str::FromStr, time::Duration};

use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use error::Error;
use futures::stream::StreamExt;
use serde_json::Value;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    SqlitePool,
};
use tracing::{instrument, Level};
use weblib::{
    entity::Transaction,
    logic::{CSVReader, Model},
    query::SqliteStore,
};

mod error;

async fn setup_database() -> SqlitePool {
    let root = project_root::get_project_root()
        .map(|r| r.join("sqlite@localhost/sqlite.db"))
        .unwrap();
    let connections_options = SqliteConnectOptions::from_str(root.to_str().unwrap())
        .expect("database does not exist")
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(30));

    let pool = SqlitePoolOptions::new()
        .max_connections(50)
        .acquire_timeout(Duration::from_secs(30))
        .connect_with(connections_options)
        .await
        .expect("can't connect to database");

    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("cannot migrate db");

    pool
}

fn application(pool: SqlitePool) -> Router {
    Router::new()
        .route("/report", get(report))
        .route("/transactions", post(transactions))
        .with_state(pool)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();

    let pool = setup_database().await;
    let app = application(pool);

    let addr = SocketAddr::from(([127, 0, 0, 1], 5000));
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .expect("can't start server");

    tracing::debug!("listening on {}", addr);
}

#[instrument(skip(pool))]
async fn report(State(pool): State<SqlitePool>) -> Result<Json<Value>, Error> {
    let tx = pool.begin().await?;

    let mut store = SqliteStore::from_sqlite_transaction(tx);
    let reports = store.get_reports().await?;
    let report = Model::calculate_total_report(reports.iter());

    Ok(Json(serde_json::to_value(report).unwrap()))
}

#[instrument(skip(pool, multipart))]
async fn transactions(
    State(pool): State<SqlitePool>,
    mut multipart: Multipart,
) -> Result<StatusCode, Error> {
    const KEY: &str = "data";
    while let Some(field) = multipart.next_field().await? {
        let name = field.name();
        match name {
            Some(name) if name == KEY => {
                let data = field.bytes().await?;
                let transactions = CSVReader::read_transaction_from_csv_bytes(data.as_ref());
                let transactions: Vec<Transaction> = transactions.collect().await;
                let tx = pool.begin().await?;
                tracing::debug!("entering critical section");
                let sqlite_store = SqliteStore::from_sqlite_transaction(tx);
                Model::commit_transactions(&transactions, sqlite_store).await?;
                return Ok(StatusCode::CREATED);
            }
            _ => (),
        }
    }

    Err(Error(anyhow::anyhow!(
        "no valid CSV with key field *{}* inside POST",
        KEY
    )))
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use sqlx::SqlitePool;
    use weblib::entity::Report;

    use crate::application;
    use tower::ServiceExt;

    #[sqlx::test]
    async fn get_report(pool: SqlitePool) -> Result<(), super::error::Error> {
        let app = application(pool);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/report")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let expected_report = Report::new();

        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let report: Report = serde_json::from_slice(&body).unwrap();

        assert_eq!(expected_report, report);
        Ok(())
    }
}
