use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    QueryError(#[from] sqlx::Error),
    #[error("{0}")]
    QueryErrorBuilding(#[from] sea_query::error::Error),
    #[error("Invalid CSV income entry")]
    InvalidCSVIncome,
}
