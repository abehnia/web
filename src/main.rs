use std::{net::SocketAddr, str::FromStr};

use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use derive_builder::Builder;
use futures::stream::StreamExt;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{
    sqlite::{SqlitePoolOptions, SqliteRow},
    Acquire, FromRow, QueryBuilder, Row, Sqlite, SqlitePool,
};
use tracing::Level;
use uuid::Uuid;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();

    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .connect("sqlite://sqlite@localhost/sqlite.db")
        .await
        .expect("can't connect to database");

    let app = Router::new()
        .route("/report", get(handler))
        .route("/transactions", post(transactions))
        .with_state(pool);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    tracing::debug!("listening on {}", addr);
}

#[derive(Serialize, Deserialize)]
struct Report {
    gross_revenue: Decimal,
    expenses: Decimal,
    net_revenue: Decimal,
}

impl Report {
    pub fn add_transaction(&mut self, transaction: &Transaction) {
        if transaction.income {
            self.gross_revenue += transaction.amount;
            self.net_revenue += transaction.amount;
        } else {
            self.expenses += transaction.amount;
            self.net_revenue -= transaction.amount;
        }
    }
}

impl FromRow<'_, SqliteRow> for Report {
    fn from_row(row: &SqliteRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            gross_revenue: Decimal::from_str(row.try_get("gross_revenue").unwrap()).unwrap(),
            expenses: Decimal::from_str(row.try_get("expenses").unwrap()).unwrap(),
            net_revenue: Decimal::from_str(row.try_get("net_revenue").unwrap()).unwrap(),
        })
    }
}

#[derive(Debug, Deserialize)]
struct TransactionFromCSV {
    date: String,
    income: String,
    amount: Decimal,
    memo: String,
}

#[derive(Debug, Deserialize, Builder)]
struct Transaction {
    id: Uuid,
    date: String,
    income: bool,
    amount: Decimal,
    memo: String,
}

impl From<TransactionFromCSV> for Transaction {
    fn from(value: TransactionFromCSV) -> Self {
        TransactionBuilder::default()
            .id(Uuid::new_v4())
            .date(value.date)
            .income("income" == value.income.to_lowercase())
            .amount(value.amount)
            .memo(value.memo)
            .build()
            .unwrap()
    }
}

async fn handler(State(pool): State<SqlitePool>) -> Result<Json<Value>, (StatusCode, String)> {
    let report: Report = sqlx::query_as("SELECT * FROM report")
        .fetch_one(&pool)
        .await
        .unwrap();

    Ok(Json(serde_json::to_value(report).unwrap()))
}

async fn transactions(State(pool): State<SqlitePool>, mut multipart: Multipart) -> StatusCode {
    while let Some(field) = multipart.next_field().await.unwrap() {
        let name = field.name();
        if let Some(name) = name {
            if name != "data" {
                continue;
            }
            let data = field.bytes().await.unwrap();
            let csv_reader = csv_async::AsyncDeserializer::from_reader(data.as_ref());
            let mut records = csv_reader.into_deserialize::<TransactionFromCSV>();
            let mut vec: Vec<Transaction> = vec![];
            while let Some(record) = records.next().await {
                let transacton_from_csv = record.unwrap();
                tracing::debug!("{:?}", transacton_from_csv);
                let transaction: Transaction = transacton_from_csv.into();
                vec.push(transaction);
            }

            let mut tx = pool.begin().await.unwrap();
            let mut report: Report = sqlx::query_as("SELECT * FROM report")
                .fetch_one(&pool)
                .await
                .unwrap();
            for transaction in &vec {
                report.add_transaction(transaction);
            }

            let mut query_builder: QueryBuilder<'_, Sqlite> =
                QueryBuilder::new("INSERT INTO transactions (id, date, income, amount, memo) ");
            query_builder.push_values(vec, |mut b, transaction| {
                b.push_bind(transaction.id.to_string())
                    .push_bind(transaction.date)
                    .push_bind(transaction.income)
                    .push_bind(transaction.amount.to_string())
                    .push_bind(transaction.memo);
            });
            let query = query_builder.build();
            query.execute(&mut *tx).await.unwrap();
            sqlx::query(
                "UPDATE report SET gross_revenue = ?, expenses = ?, net_revenue = ? WHERE id = 0",
            )
            .bind(report.gross_revenue.to_string())
            .bind(report.expenses.to_string())
            .bind(report.net_revenue.to_string())
            .execute(&mut *tx)
            .await
            .unwrap();
            tx.commit().await.unwrap();
        }
    }
    StatusCode::CREATED
}
