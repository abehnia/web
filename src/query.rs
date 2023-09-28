use sea_query::{Iden, Query, SqliteQueryBuilder};
use sea_query_binder::SqlxBinder;
use sqlx::Sqlite;
use tracing::instrument;

use crate::{
    entity::{self, Transaction, WithId},
    error::Error,
};

#[derive(Iden)]
enum Report {
    Table,
    Id,
    GrossRevenue,
    Expenses,
}

#[derive(Iden)]
enum Transactions {
    Table,
    Id,
    Date,
    Amount,
    Memo,
}

#[derive(Debug)]
pub struct SqliteStore<'a> {
    transaction: sqlx::Transaction<'a, Sqlite>,
}

impl<'a> SqliteStore<'a> {
    #[must_use]
    pub fn from_sqlite_transaction(transaction: sqlx::Transaction<'a, Sqlite>) -> SqliteStore {
        SqliteStore { transaction }
    }

    #[instrument(skip(self))]
    pub async fn get_reports(&mut self) -> Result<Vec<entity::Report>, Error> {
        let (query, values) = Query::select()
            .columns([Report::GrossRevenue, Report::Expenses])
            .from(Report::Table)
            .build_sqlx(SqliteQueryBuilder);

        Ok(sqlx::query_as_with::<_, entity::Report, _>(&query, values)
            .fetch_all(&mut *self.transaction)
            .await?)
    }

    #[instrument(skip(self))]
    pub async fn create_report(
        &mut self,
        WithId { id, data }: &WithId<entity::Report>,
    ) -> Result<(), Error> {
        let report = &data;
        let (query, values) = Query::insert()
            .into_table(Report::Table)
            .columns([Report::Id, Report::GrossRevenue, Report::Expenses])
            .values([
                id.to_string().into(),
                report.gross_revenue.into(),
                report.expenses.into(),
            ])?
            .build_sqlx(SqliteQueryBuilder);

        sqlx::query_with(&query, values)
            .execute(&mut *self.transaction)
            .await
            .map_err(Error::QueryError)
            .map(|_| ())
    }

    #[instrument(skip(self))]
    async fn get_no_transactions(&mut self) -> Result<usize, Error> {
        let mut query_builder = Query::select();
        query_builder.from(Transactions::Table).columns([
            Transactions::Id,
            Transactions::Date,
            Transactions::Amount,
            Transactions::Memo,
        ]);

        let (transactions_query, transactions_values) =
            query_builder.build_sqlx(SqliteQueryBuilder);

        sqlx::query_with(&transactions_query, transactions_values)
            .fetch_all(&mut *self.transaction)
            .await
            .map_err(Error::QueryError)
            .map(|x| x.len())
    }

    #[instrument(skip(self, transactions))]
    pub async fn create_transactions(
        &mut self,
        transactions: impl IntoIterator<Item = WithId<&Transaction>>,
    ) -> Result<(), Error> {
        let mut query_builder = Query::insert();
        query_builder.into_table(Transactions::Table).columns([
            Transactions::Id,
            Transactions::Date,
            Transactions::Amount,
            Transactions::Memo,
        ]);

        for transaction in transactions {
            let id = transaction.id;
            let data = &transaction.data;
            query_builder.values([
                id.to_string().into(),
                data.date.to_string().into(),
                data.amount.into(),
                data.memo.clone().into(),
            ])?;
        }

        let (transactions_query, transactions_values) =
            query_builder.build_sqlx(SqliteQueryBuilder);

        sqlx::query_with(&transactions_query, transactions_values)
            .execute(&mut *self.transaction)
            .await
            .map_err(Error::QueryError)
            .map(|_| ())
    }

    /// # Errors
    ///
    pub async fn commit(self) -> Result<(), Error> {
        self.transaction.commit().await.map_err(Error::QueryError)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use chrono::NaiveDate;
    use rust_decimal_macros::dec;
    use sqlx::SqlitePool;

    use crate::{
        entity::{Report, Transaction, WithId},
        error,
        query::SqliteStore,
    };

    #[sqlx::test]
    async fn empty_report(pool: SqlitePool) -> Result<(), error::Error> {
        let tx = pool.begin().await?;
        let mut sqlite_store = SqliteStore::from_sqlite_transaction(tx);

        let reports = sqlite_store.get_reports().await?;

        assert_eq!(reports.len(), 0);
        Ok(())
    }

    #[sqlx::test]
    async fn update_database(pool: SqlitePool) -> Result<(), error::Error> {
        let tx = pool.begin().await?;
        let mut sqlite_store = SqliteStore::from_sqlite_transaction(tx);

        let transactions = vec![
            Transaction {
                date: NaiveDate::from_str("2021-07-12").unwrap(),
                amount: dec!(87.32),
                memo: "first".to_string(),
            },
            Transaction {
                date: NaiveDate::from_str("2023-08-20").unwrap(),
                amount: dec!(-12.13),
                memo: "second".to_string(),
            },
        ];

        let _ = sqlite_store
            .create_transactions(transactions.iter().map(WithId::from_data))
            .await?;
        let no_transactions = sqlite_store.get_no_transactions().await?;

        assert_eq!(no_transactions, transactions.len());
        Ok(())
    }

    #[sqlx::test]
    async fn add_report(pool: SqlitePool) -> Result<(), error::Error> {
        let tx = pool.begin().await?;
        let mut sqlite_store = SqliteStore::from_sqlite_transaction(tx);

        let expected_report = Report {
            gross_revenue: dec!(20.00),
            expenses: dec!(15.12),
            net_revenue: dec!(4.88),
        };

        let with_id = WithId::from_data(expected_report.clone());
        let _ = sqlite_store.create_report(&with_id).await?;
        let reports = sqlite_store.get_reports().await?;

        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0], expected_report);
        Ok(())
    }
}
