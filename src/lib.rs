#![doc(
    html_logo_url = "https://raw.githubusercontent.com/Hareas/modus/f84c842b49b7dbbfa4b8f8acb6122d5dc5d92a3b/logo.svg"
)]
//!
//! Long term portfolio performance and option valuation.
//!
//! This library has two main purposes:
//!
//! To provide portfolio performance from historical data, irrespective of the amount invested.
//!
//! To calculate option value and provide optimal betting size

mod yahoo_finance {
    use chrono::DateTime;
    use curl::easy::{Easy, List};
    use curl::Error;
    use modus_derive::From;
    use time::OffsetDateTime;
    use yahoo_finance_api::{Quote, YResponse, YahooError};

    /// This custom error uses the custom derive macro From to implement the From trait
    ///
    /// Example:
    /// ```
    ///  impl From<YahooError> for ProviderError {
    ///      fn from (_e: YahooError) -> Self {
    ///          ProviderError::YahooError
    ///      }
    ///  }
    /// ```
    #[derive(From)]
    pub enum ProviderError {
        Error,
        YahooError,
    }

    async fn fuck_429(
        ticker: &str,
        start: &OffsetDateTime,
        end: &OffsetDateTime,
    ) -> Result<YResponse, ProviderError> {
        let start = start.unix_timestamp();
        let end = end.unix_timestamp();
        let mut easy = Easy::new();
        easy.url(&format!("https://query1.finance.yahoo.com/v8/finance/chart/{ticker}?symbol={ticker}&period1={start}&period2={end}&interval=1d&events=div%7Csplit%7CcapitalGains")).unwrap();
        let mut list = List::new();
        list.append("user-agent: curl/7.68.0")?;
        easy.http_headers(list)?;
        let mut response_data = Vec::new();
        {
            let mut transfer = easy.transfer();
            transfer.write_function(|data| {
                response_data.extend_from_slice(data);
                Ok(data.len())
            })?;
            transfer.perform()?;
        }
        let response_string = serde_json::from_slice(&response_data).unwrap_or_default();

        Ok(YResponse::from_json(response_string)?)
    }

    async fn yahoo_it(
        ticker: &str,
        start: &OffsetDateTime,
        end: &OffsetDateTime,
    ) -> Result<Vec<Quote>, ProviderError> {
        // returns historic quotes with daily interval
        let provider = fuck_429(&ticker, start, end).await?;
        // gets the currency the data is in
        let currency = provider.metadata()?.currency;
        // converts the adjclose to USD
        match currency.as_str() {
            "USD" => Ok(provider.quotes()?),
            _ => {
                // returns the exchange rate for the relevant period
                let currency_quotes = fuck_429(&format!("{}=X", currency), start, end)
                    .await?
                    .quotes()?;
                // applies the exchange rate to adjclose
                let usd_quotes: Vec<Quote> = provider
                    .quotes()?
                    .iter()
                    .map(|q| {
                        let currency_quote = currency_quotes.iter().find(|x| {
                            DateTime::from_timestamp(x.timestamp as i64, 0)
                                .unwrap_or_default()
                                .date_naive()
                                == DateTime::from_timestamp(q.timestamp as i64, 0)
                                    .unwrap_or_default()
                                    .date_naive()
                        });
                        Quote {
                            adjclose: q.adjclose
                                * currency_quote
                                    .unwrap_or_else(|| currency_quotes.last().unwrap())
                                    .adjclose,
                            ..*q
                        }
                    })
                    .collect();
                Ok(usd_quotes)
            }
        }
    }

    pub async fn get_quotes(
        ticker: &str,
        start: &OffsetDateTime,
        end: &OffsetDateTime,
    ) -> Result<Vec<Quote>, ProviderError> {
        yahoo_it(ticker, start, end).await
    }

    // returns the exchange rate at a specific date
    async fn price_at_date(ticker: &str, date: &OffsetDateTime) -> Result<f64, ProviderError> {
        if let Some(c) = fuck_429(&format!("{}=X", ticker), date, date)
            .await?
            .quotes()?
            .first()
        {
            Ok(c.close)
        } else {
            Err(ProviderError::YahooError)
        }
    }

    // returns the exchange rate with respect to the USD
    pub async fn check_currency(ticker: &str, date: &OffsetDateTime) -> Result<f64, ProviderError> {
        if let Ok(s) = fuck_429(
            ticker,
            &OffsetDateTime::now_utc(),
            &OffsetDateTime::now_utc(),
        )
        .await
        {
            if let Ok(r) = s.metadata() {
                if r.currency.as_str().ne("USD") {
                    return price_at_date(r.currency.as_str(), date).await;
                }
            };
        };
        Ok(1.0)
    }
}

pub mod stock_returns {
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

    use std::collections::{BTreeMap, HashSet};

    use chrono::{DateTime, NaiveDate};
    pub use modus_derive::From;
    use serde::{Deserialize, Serialize};
    use time::error::ComponentRange;
    use time::macros::time;
    use time::{Date, Month, OffsetDateTime};
    use yahoo_finance_api::Quote;

    use crate::yahoo_finance::{check_currency, get_quotes, ProviderError};

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
                    Date::from_calendar_date(
                        sell.date.year,
                        sell.date.match_month(),
                        sell.date.day,
                    )
                    .unwrap_or(Date::MIN),
                    time!(23:59:59),
                )
            })
            .unwrap_or_else(OffsetDateTime::now_utc);
        Ok((start, end))
    }

    // returns a Result<HashSet<NaiveDate>, StocksError> where the Ok variant is a HashSet with all the holidays
    async fn find_holidays(item: &Portfolio) -> Result<HashSet<NaiveDate>, StocksError> {
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
            let mut seen = HashSet::new();
            for timestamp in every_timestamp {
                let date = DateTime::from_timestamp(timestamp as i64, 0)
                    .unwrap_or_default()
                    .date_naive();
                // inserts the date into the HashSet, if it can't, removes the existing one from the HashSet without replacing it
                if !seen.insert(date) {
                    seen.remove(&date);
                }
            }
            Ok(seen)
        }
    }

    /// Returns a Result<BTreeMap<String, f64>, StocksError> where the BTreeMap is composed of a date as key and a percentage gain as value
    /// and StocksError is an enum with the different types of Error that might have occurred
    pub async fn total_returns(item: &Portfolio) -> Result<BTreeMap<String, f64>, StocksError> {
        // a BTreeMap because the data should be ordered by key
        let mut returns = BTreeMap::new();
        let holidays = find_holidays(item).await?;
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
            for (i, m) in quotes.iter().enumerate() {
                // converts the date from a timestamp to a NaiveDate for a more human-readable YYYY-MM-DD
                let date = DateTime::from_timestamp(m.timestamp as i64, 0)
                    .unwrap_or_default()
                    .date_naive();
                // checks if it's 5pm somewhere, if it is, grabs a beer
                if !holidays.contains(&date) {
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
                }
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
}

pub mod options {
    //! Option valuation and betting optimization
    //!
    //! # Black-Scholes formula
    //! Calculates the value of a European-type option using the [Black-Scholes formula](https://en.wikipedia.org/wiki/Black%E2%80%93Scholes_model#Black%E2%80%93Scholes_formula).
    //! Note that this is also valid for American-type call options but not for American-type put options, as shown by [Merton (1973)](https://doi.org/10.2307/1913811)
    //! provided the stock does not pay dividends.
    //! Because it uses the Black-Scholes formula, it has the same limitations, chiefly among them, the constant volatility
    //!
    //! # Usage:
    //! ```
    //!  let a_option = Options{
    //!     form: OptionType::Call,
    //!     underlying: 43.0,
    //!     strike: 55.0,
    //!     maturity: 3,
    //!     volatility: 0.7,
    //!     rfr: 0.3,
    //!     market_price: None,
    //!  };
    //!  println!("{}", bs_price(&a_option));
    //! ```
    //!
    //! # Monte-Carlo analysis
    //! Alternatively, it performs a [Monte-Carlo analysis](https://en.wikipedia.org/wiki/Monte_Carlo_method) to calculate the option price.
    //!
    //! # Usage:
    //! ```
    //! let a_option = Options{
    //!     form: OptionType::Call,
    //!     underlying: 43.0,
    //!     strike: 55.0,
    //!     maturity: 3,
    //!     volatility: 0.7,
    //!     rfr: 0.3,
    //!     market_price: None,
    //!  };
    //!  if let Ok(s) = expected(&a_option) { println!("{:?}", s); }
    //! ```
    //!
    //! # Kelly Criterion
    //! If one were to be able to consistently find theoretical market values of the options different from their market values one could design an optimal strategy where the
    //! expected geometric growth rate is maximized by finding the fraction of the bankroll that maximizes the expected value of the logarithm of wealth, also known as the
    //! [Kelly Criterion](https://en.wikipedia.org/wiki/Kelly_criterion).
    //! This tries to implement the Kelly Criterion to find that fraction for a given option. I'm not fully certain the implementation is correct however, so you might want to
    //! consider a more mature crate for this.
    //!
    //! # Usage:
    //! ```
    //! let a_option = Options{
    //!     form: OptionType::Call,
    //!     underlying: 43.0,
    //!     strike: 55.0,
    //!     maturity: 3,
    //!     volatility: 0.7,
    //!     rfr: 0.3,
    //!     market_price: Some(19.0),
    //!  };
    //!  if let Some(s) = kelly_ratio(&a_option) { println!("{:?}", s); }
    //! ```

    use std::sync::mpsc::RecvError;
    use std::sync::{mpsc, Arc};
    use std::thread;

    use rstat::univariate::normal::Normal;
    use rstat::Distribution;
    use serde::{Deserialize, Serialize};

    /// Holds the option data
    #[derive(Debug, Serialize, Deserialize, Copy, Clone)]
    pub struct Options {
        form: OptionType,
        underlying: f64,
        strike: f64,
        maturity: u8,
        volatility: f64,
        rfr: f64,
        market_price: Option<f64>,
    }

    #[derive(Debug, Serialize, Deserialize, Copy, Clone)]
    enum OptionType {
        Call,
        Put,
    }

    /// Calculates the option value with the Black-Scholes formula
    pub fn bs_price(item: &Options) -> f64 {
        let d1 = d1(item);
        let d2 = d2(d1, item);
        match item.form {
            OptionType::Call => {
                item.underlying * Normal::standard().cdf(&d1)
                    - item.strike
                        * (-item.rfr * item.maturity as f64).exp()
                        * Normal::standard().cdf(&d2)
            }
            OptionType::Put => {
                item.strike
                    * (-item.rfr * item.maturity as f64).exp()
                    * Normal::standard().cdf(&-d2)
                    - item.underlying * Normal::standard().cdf(&-d1)
            }
        }
    }

    fn d1(item: &Options) -> f64 {
        ((item.underlying / item.strike).ln()
            + (item.rfr + (item.volatility.powi(2) / 2.0)) * item.maturity as f64)
            / (item.volatility * (item.maturity as f64).sqrt())
    }

    fn d2(d1: f64, item: &Options) -> f64 {
        d1 - item.volatility * (item.maturity as f64).sqrt()
    }

    /// Calculates the Kelly fraction
    pub fn kelly_ratio(item: &Options) -> Option<f64> {
        let d1 = d1(item);
        let d2 = d2(d1, item);
        let w = (bs_price(item) / Normal::standard().cdf(&d2) - item.market_price?)
            / item.market_price?;
        Some((Normal::standard().cdf(&d2) * w - (1.0 - Normal::standard().cdf(&d2))) / w)
    }

    /// Performs a Monte-Carlo analysis with 10000 simulations
    pub fn expected(item: &Options) -> Result<f64, RecvError> {
        // an arc because the value is immutable between threads
        let values = Arc::new(*item);
        let (tx, rx) = mpsc::channel();
        for _ in 0..10000 {
            let (values, tx) = (values.clone(), tx.clone());
            thread::spawn(move || {
                let data = values.underlying
                    * ((values.rfr - values.volatility.powi(2) / 2.0) * values.maturity as f64
                        + values.volatility
                            * (values.maturity as f64).sqrt()
                            * Normal::standard().sample(&mut rand::thread_rng()))
                    .exp();
                tx.send(data)
            });
        }
        let mut v: Vec<f64> = Vec::new();
        // receives the result of an iteration and propagates it
        for _ in 0..10000 {
            v.push(rx.recv()?);
        }
        // calculates the return for each iteration
        let returns: Vec<f64> = v
            .iter()
            .map(|&x| match item.form {
                OptionType::Call => match x <= item.strike {
                    true => 0.0,
                    false => (x - item.strike) / (1.0 + item.rfr).powi(item.maturity as i32),
                },
                OptionType::Put => match x >= item.strike {
                    true => 0.0,
                    false => (item.strike - x) / (1.0 + item.rfr).powi(item.maturity as i32),
                },
            })
            .collect();
        // computes the average
        Ok(returns.iter().sum::<f64>() / returns.len() as f64)
    }
}
