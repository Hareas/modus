//! Portfolio performance
//!
//! Most calculations of portfolio performance don't include the whole data or are affected by when an asset is bought and therefore are not suitable for comparison.
//! This module shows performance controlling for those factors.
//!
//! The function total_returns takes a Portfolio and returns a Result<BTreeMap<String, f64>, StocksError>,
//! StocksError being a custom error enum for the error types that can occur.
//!
//! Usage:
//! ```
//!  let portfolio = Portfolio{portfolio: vec![Equity{ticker: "MSFT".to_string(), buy: Transaction { date: TransactionDate {
//!         year: 2023,
//!         month: 2,
//!         day: 1,
//!     }, price: 354.0 }, sell: None, quantity: 3 }]};
//!  if let Ok(s) = total_returns(&portfolio).await { println!("{:?}", s); }
//! ```

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, NaiveDate};
pub use modus_derive::From;
use serde::{Deserialize, Serialize};
use time::error::ComponentRange;
use time::macros::time;
use time::{Date, Month, OffsetDateTime};

use crate::yahoo_finance::{check_currency, get_quotes, ProviderError, Quote};

#[derive(Debug, Serialize, Deserialize)]
struct Position {
    old_price: f64,
    price: f64,
    quantity: u32,
}

/// Holds the historical data about your portfolio
#[derive(Debug, Serialize, Deserialize)]
pub struct Portfolio {
    portfolio: Vec<Equity>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Equity {
    ticker: String,
    buy: Transaction,
    sell: Option<Transaction>,
    quantity: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct Transaction {
    date: TransactionDate,
    price: f64,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
struct TransactionDate {
    year: i32,
    month: u32,
    day: u8,
}

impl TransactionDate {
    fn match_month(&self) -> Month {
        match self.month {
            1 => Month::January,
            2 => Month::February,
            3 => Month::March,
            4 => Month::April,
            5 => Month::May,
            6 => Month::June,
            7 => Month::July,
            8 => Month::August,
            9 => Month::September,
            10 => Month::October,
            11 => Month::November,
            12 => Month::December,
            _ => Month::January,
        }
    }
}

/// This custom error uses the custom derive macro From to implement the From trait
///
/// Example:
/// ```
///  impl From<ComponentRange> for StocksError {
///      fn from (_e: ComponentRange) -> Self {
///          StocksError::ComponentRange
///      }
///  }
/// ```
#[derive(From)]
pub enum StocksError {
    ComponentRange,
    ProviderError,
}

// the Ok variant is a range with dates in YYYY-MM_DD
fn get_range(n: &Equity) -> Result<(OffsetDateTime, OffsetDateTime), ComponentRange> {
    let start = OffsetDateTime::new_utc(
        Date::from_calendar_date(n.buy.date.year, n.buy.date.match_month(), n.buy.date.day)?,
        time!(0:00:00),
    );
    let end = n
        .sell
        .as_ref()
        .map(|sell| {
            OffsetDateTime::new_utc(
                Date::from_calendar_date(sell.date.year, sell.date.match_month(), sell.date.day)
                    .unwrap_or(Date::MIN),
                time!(23:59:59),
            )
        })
        .unwrap_or_else(OffsetDateTime::now_utc);
    Ok((start, end))
}

// returns a Result<HashSet<NaiveDate>, StocksError> where the Ok variant is a HashSet with all the holidays
async fn find_dates(item: &Portfolio) -> Result<BTreeSet<NaiveDate>, StocksError> {
    {
        let mut range: Vec<(OffsetDateTime, OffsetDateTime)> = Vec::new();
        for n in item.portfolio.iter() {
            let (start, end) = get_range(n)?;
            range.push((start, end));
        }
        // finds the earliest and latest date and assigns them to start and end, respectively
        let (start, end) = range
            .iter()
            .fold((range[0].0, range[0].1), |(s, e), (rs, re)| {
                (s.min(*rs), e.max(*re))
            });
        let mut historical_data: Vec<Vec<Quote>> = Vec::new();
        for n in item.portfolio.iter() {
            historical_data.push(get_quotes(&n.ticker, &start, &end).await?);
        }
        let every_timestamp = historical_data
            .iter()
            .flat_map(|f| f.iter().map(|g| g.timestamp));
        let mut every_date = BTreeSet::new();
        for timestamp in every_timestamp {
            let date = DateTime::from_timestamp(timestamp as i64, 0)
                .unwrap_or_default()
                .date_naive();
            // inserts the date into the HashSet, if it can't, removes the existing one from the HashSet without replacing it
            every_date.insert(date);
        }
        Ok(every_date)
    }
}

/// Returns a Result<BTreeMap<String, f64>, StocksError> where the BTreeMap is composed of a date as key and a percentage gain as value
/// and StocksError is an enum with the different types of Error that might have occurred
pub async fn total_returns(item: &Portfolio) -> Result<BTreeMap<String, f64>, StocksError> {
    // a BTreeMap because the data should be ordered by key
    let mut returns = BTreeMap::new();
    let every_date = find_dates(item).await?;
    // iterates over every element in the portfolio
    for n in item.portfolio.iter() {
        let (start, end) = get_range(n)?;
        // exchange rate at the buy and end dates to convert them
        let start_currency_adjustment = check_currency(&n.ticker, &start).await?;
        let end_currency_adjustment = check_currency(&n.ticker, &end).await?;
        // buy price in USD at the date of buying
        let mut old_price = n.buy.price * start_currency_adjustment;
        // sets price to the price in USD at the time of selling
        let adjusted_selling_data: Option<Transaction> = n.sell.as_ref().map(|s| Transaction {
            price: s.price * end_currency_adjustment,
            ..*s
        });
        // returns all the quotes for that ticker in the specified range
        let quotes = get_quotes(&n.ticker, &start, &end).await?;
        let mut previous_date = NaiveDate::MIN;
        for (i, m) in quotes.iter().enumerate() {
            // converts the date from a timestamp to a NaiveDate for a more human-readable YYYY-MM-DD
            let date = DateTime::from_timestamp(m.timestamp as i64, 0)
                .unwrap_or_default()
                .date_naive();
            // checks if it's 5pm somewhere, if it is, grabs a beer
            if i > 0 {
                let previous_index = every_date
                    .iter()
                    .position(|&last_date| last_date == previous_date)
                    .unwrap();
                let current_index = every_date.iter().position(|&now| now == date).unwrap();
                if current_index - previous_index > 1 {
                    for missing_date_index in (previous_index + 1)..current_index {
                        let missing_date = every_date.iter().nth(missing_date_index).unwrap();
                        returns
                            .entry(*missing_date)
                            .or_insert_with(Vec::new)
                            .push(Position {
                                // if it's the last quote, weights the old price by the difference between the close and adjclose to avoid distortions...
                                old_price: old_price * m.close / m.adjclose,
                                // ... and sets the selling price in USD if it has been sold and does the same weighting or keeps the adjclose otherwise
                                price: old_price * m.close / m.adjclose,
                                quantity: n.quantity,
                            });
                    }
                }
            }
            returns
                .entry(date)
                .or_insert_with(Vec::new)
                .push(if i == quotes.len() - 1 {
                    Position {
                        // if it's the last quote, weights the old price by the difference between the close and adjclose to avoid distortions...
                        old_price: old_price * m.close / m.adjclose,
                        // ... and sets the selling price in USD if it has been sold and does the same weighting or keeps the adjclose otherwise
                        price: adjusted_selling_data
                            .as_ref()
                            .map(|sell| sell.price * m.close / m.adjclose)
                            .unwrap_or_else(|| m.adjclose),
                        quantity: n.quantity,
                    }
                } else if i == 0 {
                    Position {
                        // if it's the first quote weights the old price and the price (buy price in this case) as previously described
                        old_price: old_price * m.close / m.adjclose,
                        price: m.close * start_currency_adjustment * m.close / m.adjclose,
                        quantity: n.quantity,
                    }
                } else {
                    Position {
                        old_price,
                        price: m.adjclose,
                        quantity: n.quantity,
                    }
                });
            // if the next quote is the last, sets the old price as the close price converted to USD by the exchange rate
            old_price = if i == quotes.len() - 2 {
                m.close * end_currency_adjustment
            } else {
                m.adjclose
            };
            previous_date = date;
        }
    }
    let mut cumulative: f64 = 1.0;
    Ok(returns
        .iter()
        .map(|(date, positions)| {
            (date.to_string(), {
                // calculates the total value of every position at the beginning of the day and sums it up for every day
                let cap = positions
                    .iter()
                    .fold(0.0, |acc, pos| acc + pos.old_price * pos.quantity as f64);
                // calculates the value of every position at the end of the day and divides it by the total value at the beginning of the day and sums it up for every day
                positions
                    .iter()
                    .fold(0.0, |acc, pos| acc + pos.price * pos.quantity as f64 / cap)
            })
        })
        // transforms the daily aggregate growth into continuous growth in percentage
        .map(|(date, rate)| {
            (date, {
                cumulative *= rate;
                (cumulative - 1.0) * 100.0
            })
        })
        .collect())
}
