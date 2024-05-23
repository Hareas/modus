//! Long term portfolio performance and option valuation.
//!
//! This library has two main functions:
//! 
//! To provide portfolio performance from historical data, irrespective of the amount invested.
//! 
//! To calculate option value and provide optimal betting size

mod yahoo_finance {
    use time::OffsetDateTime;
    use yahoo_finance_api as yahoo;
    use yahoo_finance_api::{Quote, YahooError};

    async fn yahoo_it (ticker: &str, start: &OffsetDateTime, end: &OffsetDateTime) -> Result<Vec<Quote>, YahooError> {
        let provider = yahoo::YahooConnector::new();
        // returns historic quotes with daily interval
        provider.get_quote_history(ticker, *start, *end).await?.quotes()
    }
    
    pub async fn get_quotes (ticker: &str, start: &OffsetDateTime, end: &OffsetDateTime) -> Result<Vec<Quote>, YahooError> {
        yahoo_it(ticker, start, end).await
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
    //!     match total_returns(&item).await {
    //!         Ok(res) => HttpResponse::Ok().json(res),
    //!         Err(e) => match e {
    //!             StocksError::ComponentRange => { HttpResponse::BadRequest().json(json!({"Error": "Failed to convert the date"})) }
    //!             StocksError::YahooError => { HttpResponse::InternalServerError().json(json!({"Error": "Yahoo provided a wrong response or didn't respond"})) }
    //!         }
    //!     }
    //! ```

    use std::collections::BTreeMap;
    use chrono::DateTime;
    pub use modus_derive::From;
    use serde::{Deserialize, Serialize};
    use time::{Date, Month, OffsetDateTime};
    use time::error::ComponentRange;
    use time::macros::time;
    use yahoo_finance_api::YahooError;
    use crate::yahoo_finance::get_quotes;

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

    #[derive(Debug, Serialize, Deserialize)]
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
        YahooError
    }

    /// Returns a Result<BTreeMap<String, f64>, StocksError> where the BTreeMap is composed of a date as key and a percentage gain as value
    /// and StocksError is a enum with the different types of Error that might have occurred
    pub async fn total_returns (item: &Portfolio) -> Result<BTreeMap<String, f64>, StocksError>{
        let mut returns = BTreeMap::new();
        for n in item.portfolio.iter() {
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
                            .unwrap(),
                        time!(23:59:59),
                    )
                })
                .unwrap_or_else(OffsetDateTime::now_utc);
            let mut old_price = n.buy.price;
            let quotes = get_quotes(&n.ticker, &start, &end).await?;
            for (i, m) in quotes.iter().enumerate() {
                returns
                    .entry(
                        DateTime::from_timestamp(m.timestamp as i64, 0)
                            .unwrap_or_default()
                            .date_naive(),
                    )
                    .or_insert_with(Vec::new)
                    .push(Position {
                        old_price,
                        price: if i == quotes.len() - 1 {
                            n.sell
                                .as_ref()
                                .map(|sell| sell.price)
                                .unwrap_or_else(|| m.adjclose)
                        } else {
                            m.adjclose
                        },
                        quantity: n.quantity,
                    });
                old_price = m.adjclose;
            }
        }
        let mut cumulative :f64 = 1.0;
        Ok(returns
            .iter()
            .map(|(date, positions)| {
                (date.to_string(), {
                    let cap = positions
                        .iter()
                        .fold(0.0, |acc, pos| acc + pos.old_price * pos.quantity as f64);
                    positions
                        .iter()
                        .fold(0.0, |acc, pos| acc + pos.price * pos.quantity as f64 / cap)
                })
            })
            .map(|(date, rate)| { (date, { cumulative *= rate;  (cumulative - 1.0) * 100.0 })})
            .collect()
        )
    }

}

pub mod options {
    //! Option valuation and betting optimization
    //!
    //! # Black-Scholes formula
    //! Calculates the value of an European-type option using the [Black-Scholes formula](https://en.wikipedia.org/wiki/Black%E2%80%93Scholes_model#Black%E2%80%93Scholes_formula).
    //! Note that this is also valid for American-type call options but not for American-type put options, as shown by [Merton (1973)](https://doi.org/10.2307/1913811)
    //! provided the stock does not pay dividends.
    //! Because it uses the Black-Scholes formula, it has the same limitations, chiefly among them, the constant volatility
    //!
    //! # Usage:
    //! ```
    //! bs_price(&item)
    //! ```
    //!     where item is of the type Options
    //!
    //! # Monte-Carlo analysis
    //! Alternatively, it performs a [Monte-Carlo analysis](https://en.wikipedia.org/wiki/Monte_Carlo_method) to calculate the option price.
    //!
    //! # Usage:
    //! ```
    //!     match expected(&item) {
    //!         Ok(res) => HttpResponse::Ok().json(json!({"Monte-Carlo value based on 10000 simulations": res})),
    //!         Err(_) => HttpResponse::Ok().json(json!({"Error": "Some iterations couldn't be completed"}))
    //!     }
    //! ```
    //!     where item is of the type Options
    //!
    //! # Kelly Criterion
    //! If one were to be able to consistently find theoretical market values of the options different from their market values one could design an optimal strategy where the
    //! expected geometric growth rate is maximized by finding the fraction of the bankroll that maximizes the expected value of the logarithm of wealth, also known as the
    //! [Kelly Critarion](https://en.wikipedia.org/wiki/Kelly_criterion).
    //! This tries to implement the Kelly Criterion to find that fraction for a given option. I'm not fully certain the implementation is correct however, so you might want to
    //! consider a more mature crate for this.
    //!
    //! # Usage:
    //! ```
    //!     match kelly_ratio(&item) {
    //!         None => HttpResponse::BadRequest().json(json!({"Error": "You haven't included the current market price"})),
    //!         Some(f) => HttpResponse::Ok().json(json!({"Kelly fraction": f}))
    //!     }
    //! ```
    //!     where item is of the type Options

    use std::sync::{Arc, mpsc};
    use std::sync::mpsc::RecvError;
    use std::thread;
    use rstat::Distribution;
    use rstat::univariate::normal::Normal;
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
        market_price: Option<f64>
    }

    #[derive(Debug, Serialize, Deserialize, Copy, Clone)]
    enum OptionType {
        Call,
        Put
    }

    /// Calculates the option value with the Black-Scholes formula
    pub fn bs_price (item: &Options) -> f64 {
        let d1 = d1(item);
        let d2 = d2(d1, item);
        match item.form {
            OptionType::Call => item.underlying * Normal::standard().cdf(&d1) - item.strike * (- item.rfr * item.maturity as f64).exp() * Normal::standard().cdf(&d2),
            OptionType::Put => item.strike * (- item.rfr * item.maturity as f64).exp() * Normal::standard().cdf(&-d2) - item.underlying * Normal::standard().cdf(&-d1)
        }
    }

    fn d1 (item: &Options) -> f64 {
        ((item.underlying / item.strike).ln() + (item.rfr + (item.volatility.powi(2)/2.0)) * item.maturity as f64) / (item.volatility * item.maturity as f64)
    }

    fn d2(d1 :f64, item :&Options) -> f64{
        d1 - item.volatility * (item.maturity as f64).sqrt()
    }

    /// Calculates the Kelly fraction
    pub fn kelly_ratio (item: &Options) -> Option<f64> {
        let d1 = d1(item);
        let d2 = d2(d1, item);
        let w = (bs_price(item) / Normal::standard().cdf(&d2) - item.market_price?) / item.market_price?;
        Some((Normal::standard().cdf(&d2) * w - (1.0 - Normal::standard().cdf(&d2))) / w)
    }

    /// Performs a Monte-Carlo analysis with 10000 simulations
    pub fn expected (item :&Options) -> Result<f64, RecvError> {
        let values = Arc::new(*item);
        let (tx, rx) = mpsc::channel();
        for _ in 0..10000 {
            let (values, tx) = (values.clone(), tx.clone());
            thread::spawn(move || {
                let data = values.underlying * ((values.rfr - values.volatility.powi(2) / 2.0) * values.maturity as f64 + values.volatility * (values.maturity as f64).sqrt() * Normal::standard().sample(&mut rand::thread_rng())).exp();
                tx.send(data)
            });
        }
        let mut v :Vec<f64> = Vec::new();
        for _ in 0..10000 {
            v.push(rx.recv()?);
        }
        let returns :Vec<f64> = v.iter()
            .map(|&x| match item.form {
                OptionType::Call => { match x <= item.strike {
                    true => 0.0,
                    false => x - item.strike
                } }
                OptionType::Put => { match x >= item.strike {
                    true => 0.0,
                    false => item.strike - x
                } }
            } ).collect();
        Ok(returns.iter().sum::<f64>() / returns.len() as f64)
    }
}