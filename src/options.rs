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
            item.strike * (-item.rfr * item.maturity as f64).exp() * Normal::standard().cdf(&-d2)
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
    let w =
        (bs_price(item) / Normal::standard().cdf(&d2) - item.market_price?) / item.market_price?;
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
