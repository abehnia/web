use csv_async::{AsyncReaderBuilder, Trim};
use futures::StreamExt;

use crate::{
    entity::{Report, Transaction, TransactionFromCSV, WithId},
    error,
    query::SqliteStore,
};

pub struct Model;

impl Model {
    pub fn calculate_balance_from_transactions<'a>(
        transactions: impl IntoIterator<Item = &'a Transaction>,
    ) -> Report {
        let mut report = Report::new();
        for transaction in transactions {
            report = Report::add_transaction(&report, transaction);
        }
        report
    }

    pub fn calculate_total_report<'a>(reports: impl IntoIterator<Item = &'a Report>) -> Report {
        let mut report = Report::new();
        for r in reports {
            report = Report::add(&report, r);
        }
        report
    }

    ///
    /// # Errors
    pub async fn commit_transactions<'a>(
        transactions: &[Transaction],
        mut sqlite_store: SqliteStore<'a>,
    ) -> Result<Report, error::Error> {
        let report = Model::calculate_balance_from_transactions(transactions);
        let report_with_id = WithId::from_data(report);

        sqlite_store
            .create_transactions(transactions.iter().map(WithId::from_data))
            .await?;
        tracing::debug!("updated transactions");

        sqlite_store.create_report(&report_with_id).await?;
        tracing::debug!("updated report");

        sqlite_store.commit().await?;
        tracing::debug!("commited");

        Ok(report)
    }
}

pub struct CSVReader;

impl CSVReader {
    #[must_use]
    pub fn read_transaction_from_csv_bytes(
        bytes: &[u8],
    ) -> impl StreamExt<Item = Transaction> + '_ {
        let csv_reader = AsyncReaderBuilder::new()
            .trim(Trim::All)
            .comment(Some(b'#'))
            .has_headers(false)
            .flexible(true)
            .create_deserializer(bytes);
        let records = csv_reader.into_deserialize::<TransactionFromCSV>();
        records
            .filter_map(|x| async move {
                if x.is_err() {
                    tracing::warn!("{:?}", x);
                };
                x.ok()
            })
            .filter_map(|x| async move {
                let x = x.try_into();
                if x.is_err() {
                    tracing::warn!("{:?}", x);
                };
                tracing::debug!("{:?}", x);
                x.ok()
            })
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use chrono::NaiveDate;
    use futures::StreamExt;
    use rust_decimal_macros::dec;
    use sqlx::SqlitePool;

    use crate::{
        entity::{Report, Transaction},
        error,
        logic::CSVReader,
        query::SqliteStore,
    };

    use super::Model;

    #[tokio::test]
    async fn valid_csv() {
        let csv = vec![
            "2021-07-12, Income, 87.32, first",
            "2023-08-20, Expense, 12.13, second",
        ]
        .join("\n");
        let expected_transactions = vec![
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

        let transactions: Vec<Transaction> =
            CSVReader::read_transaction_from_csv_bytes(csv.as_bytes())
                .collect()
                .await;

        assert_eq!(transactions, expected_transactions);
    }

    #[tokio::test]
    async fn invalid_csv() {
        let csv = vec![
            "text",
            "# comment",
            "2020-09-12, Income",
            "2021-07-12, Income, 87.32, first",
            "2023-08-13, NotExpense, 10.12, third",
            "2023-08-20, Expense, 12.13, second",
            "20-08-2023, Income, 10.00, fourth",
        ]
        .join("\n");
        let expected_transactions = vec![
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

        let transactions: Vec<Transaction> =
            CSVReader::read_transaction_from_csv_bytes(csv.as_bytes())
                .collect()
                .await;

        assert_eq!(transactions, expected_transactions);
    }

    #[test]
    fn balance_from_transactions() {
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
        let expected_report = Report {
            gross_revenue: dec!(87.32),
            expenses: dec!(12.13),
            net_revenue: dec!(75.19),
        };

        let report = Model::calculate_balance_from_transactions(transactions.iter());

        assert_eq!(report, expected_report);
    }

    #[test]
    fn total_reports() {
        let reports = vec![
            Report {
                gross_revenue: dec!(87.32),
                expenses: dec!(12.13),
                net_revenue: dec!(75.19),
            },
            Report {
                gross_revenue: dec!(10.01),
                expenses: dec!(2.05),
                net_revenue: dec!(7.96),
            },
        ];
        let expected_report = Report {
            gross_revenue: dec!(97.33),
            expenses: dec!(14.18),
            net_revenue: dec!(83.15),
        };

        let report = Model::calculate_total_report(reports.iter());

        assert_eq!(report, expected_report);
    }

    #[sqlx::test]
    async fn commit_transactions(pool: SqlitePool) -> Result<(), error::Error> {
        let tx = pool.begin().await?;
        let sqlite_store = SqliteStore::from_sqlite_transaction(tx);

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
        let expected_report = Report {
            gross_revenue: dec!(87.32),
            expenses: dec!(12.13),
            net_revenue: dec!(75.19),
        };

        let report = Model::commit_transactions(&transactions, sqlite_store).await?;

        assert_eq!(report, expected_report);

        let tx = pool.begin().await?;
        let mut sqlite_store = SqliteStore::from_sqlite_transaction(tx);

        let report_from_store = sqlite_store.get_reports().await?;

        assert_eq!(report_from_store.len(), 1);
        assert_eq!(report_from_store[0], expected_report);

        Ok(())
    }
}
