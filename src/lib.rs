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

pub mod options;
pub mod stock_returns;
mod yahoo_finance;
