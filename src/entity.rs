use std::str::FromStr;

use chrono::NaiveDate;
use derive_builder::Builder;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteRow, FromRow, Row};
use uuid::Uuid;

use crate::error;

#[derive(Debug, Serialize, Deserialize)]
pub struct WithId<T> {
    pub(crate) id: Uuid,
    pub(crate) data: T,
}

impl<T> WithId<T> {
    const ID_COL_NAME: &'static str = "id";
    pub fn from_data(data: T) -> Self {
        Self {
            id: Uuid::new_v4(),
            data,
        }
    }
}

impl<T: Default> Default for WithId<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Default> WithId<T> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            data: T::default(),
        }
    }
}

impl<'a, T: FromRow<'a, SqliteRow>> FromRow<'a, SqliteRow> for WithId<T> {
    fn from_row(row: &'a SqliteRow) -> Result<Self, sqlx::Error> {
        let id = Uuid::from_str(row.try_get(WithId::<T>::ID_COL_NAME)?).map_err(|x| {
            sqlx::Error::ColumnDecode {
                index: WithId::<T>::ID_COL_NAME.to_owned(),
                source: Box::new(x),
            }
        })?;
        let data: T = FromRow::from_row(row)?;

        Ok(Self { id, data })
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Copy)]
pub struct Report {
    pub(crate) gross_revenue: Decimal,
    pub(crate) expenses: Decimal,
    pub(crate) net_revenue: Decimal,
}

impl Report {
    const EXPENSES_COL_NAME: &'static str = "expenses";
    const GROSS_REVENUE_COL_NAME: &'static str = "gross_revenue";

    #[must_use]
    pub fn from_dec(gross_revenue: Decimal, expenses: Decimal, net_revenue: Decimal) -> Report {
        Report {
            gross_revenue,
            expenses,
            net_revenue,
        }
    }
}

impl FromRow<'_, SqliteRow> for Report {
    fn from_row(row: &SqliteRow) -> Result<Self, sqlx::Error> {
        let gross_revenue = Decimal::from_str(row.try_get(Report::GROSS_REVENUE_COL_NAME)?)
            .map_err(|x| sqlx::Error::ColumnDecode {
                index: Report::GROSS_REVENUE_COL_NAME.to_owned(),
                source: Box::new(x),
            })?;

        let expenses = Decimal::from_str(row.try_get(Report::EXPENSES_COL_NAME)?).map_err(|x| {
            sqlx::Error::ColumnDecode {
                index: Report::EXPENSES_COL_NAME.to_owned(),
                source: Box::new(x),
            }
        })?;

        Ok(Self {
            gross_revenue,
            expenses,
            net_revenue: gross_revenue - expenses,
        })
    }
}

impl Report {
    #[must_use]
    pub fn add_transaction(report: &Report, transaction: &Transaction) -> Report {
        let mut r = *report;
        if transaction.amount > dec!(0) {
            r.gross_revenue += transaction.amount;
        } else {
            r.expenses -= transaction.amount;
        }
        r.net_revenue += transaction.amount;
        r
    }

    #[must_use]
    pub fn new() -> Report {
        Report {
            gross_revenue: dec!(0),
            expenses: dec!(0),
            net_revenue: dec!(0),
        }
    }

    #[must_use]
    pub fn add(lhs: &Report, rhs: &Report) -> Report {
        let mut report = Report::new();
        report.gross_revenue = lhs.gross_revenue + rhs.gross_revenue;
        report.expenses = lhs.expenses + rhs.expenses;
        report.net_revenue = lhs.net_revenue + rhs.net_revenue;
        report
    }
}

impl Default for Report {
    fn default() -> Self {
        Report::new()
    }
}

#[derive(Debug, Deserialize)]
pub struct TransactionFromCSV {
    date: NaiveDate,
    income: String,
    amount: Decimal,
    memo: String,
}

#[derive(Debug, Deserialize, Builder, PartialEq)]
pub struct Transaction {
    pub(crate) date: NaiveDate,
    pub(crate) amount: Decimal,
    pub(crate) memo: String,
}

impl TryFrom<TransactionFromCSV> for Transaction {
    type Error = error::Error;

    fn try_from(value: TransactionFromCSV) -> Result<Self, Self::Error> {
        const INCOME: &str = "Income";
        const EXPENSE: &str = "Expense";

        let amount = if value.income == INCOME {
            value.amount
        } else if value.income == EXPENSE {
            -value.amount
        } else {
            return Err(error::Error::InvalidCSVIncome);
        };

        Ok(TransactionBuilder::default()
            .date(value.date)
            .amount(amount)
            .memo(value.memo)
            .build()
            .expect("incorrect initialization of transaction"))
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use rust_decimal_macros::dec;

    use crate::error;

    use super::{Report, Transaction, TransactionFromCSV};

    #[test]
    fn from_valid_csv_transaction() {
        let transaction_from_csv = TransactionFromCSV {
            date: NaiveDate::from_ymd_opt(2021, 7, 20).unwrap(),
            income: "Income".to_string(),
            amount: dec!(12.11),
            memo: "first".to_string(),
        };
        let expected_transaction = Transaction {
            date: NaiveDate::from_ymd_opt(2021, 7, 20).unwrap(),
            amount: dec!(12.11),
            memo: "first".to_string(),
        };

        let transaction: Transaction = TryFrom::try_from(transaction_from_csv).unwrap();

        assert_eq!(transaction, expected_transaction)
    }

    #[test]
    fn from_invalid_csv_transaction() {
        let transaction_from_csv = TransactionFromCSV {
            date: NaiveDate::from_ymd_opt(2021, 7, 20).unwrap(),
            income: "IncomeX".to_string(),
            amount: dec!(12.11),
            memo: "first".to_string(),
        };

        let transaction: Result<Transaction, _> = TryFrom::try_from(transaction_from_csv);

        assert!(matches!(transaction, Err(error::Error::InvalidCSVIncome)));
    }

    #[test]
    fn add_report() {
        let report_0 = Report {
            gross_revenue: dec!(87.32),
            expenses: dec!(12.13),
            net_revenue: dec!(75.19),
        };
        let report_1 = Report {
            gross_revenue: dec!(10.01),
            expenses: dec!(2.05),
            net_revenue: dec!(7.96),
        };
        let expected_report = Report {
            gross_revenue: dec!(97.33),
            expenses: dec!(14.18),
            net_revenue: dec!(83.15),
        };

        let report = Report::add(&report_0, &report_1);

        assert_eq!(report, expected_report);
    }

    #[test]
    fn add_transaction() {
        let transaction_0 = Transaction {
            date: NaiveDate::from_ymd_opt(2015, 11, 1).unwrap(),
            amount: dec!(87.12),
            memo: "first".to_string(),
        };
        let transaction_1 = Transaction {
            date: NaiveDate::from_ymd_opt(2016, 11, 1).unwrap(),
            amount: dec!(-12.13),
            memo: "second".to_string(),
        };

        let report = Report::new();
        let report = Report::add_transaction(&report, &transaction_0);
        let report = Report::add_transaction(&report, &transaction_1);

        let expected_report = Report {
            gross_revenue: dec!(87.12),
            expenses: dec!(12.13),
            net_revenue: dec!(74.99),
        };

        assert_eq!(report, expected_report);
    }
}
