use chrono::DateTime;
use modus_derive::From;
use reqwest::{Client, Error};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;

#[derive(Error, Debug)]
pub enum YahooError {
    #[error("fetching the data from yahoo! finance failed")]
    FetchFailed(String),
    #[error("deserializing response from yahoo! finance failed")]
    DeserializeFailed(#[from] serde_json::Error),
    #[error("connection to yahoo! finance server failed")]
    ConnectionFailed(#[from] reqwest::Error),
    #[error("yahoo! finance return invalid JSON format")]
    InvalidJson,
    #[error("yahoo! finance returned an empty data set")]
    EmptyDataSet,
    #[error("yahoo! finance returned inconsistent data")]
    DataInconsistency,
    #[error("construcing yahoo! finance client failed")]
    BuilderFailed,
}

#[derive(Deserialize, Debug)]
pub struct YResponse {
    pub chart: YChart,
}

impl YResponse {
    fn check_consistency(&self) -> Result<(), YahooError> {
        for stock in &self.chart.result {
            let n = stock.timestamp.len();
            if n == 0 {
                return Err(YahooError::EmptyDataSet);
            }
            let quote = &stock.indicators.quote[0];
            if quote.open.len() != n
                || quote.high.len() != n
                || quote.low.len() != n
                || quote.volume.len() != n
                || quote.close.len() != n
            {
                return Err(YahooError::DataInconsistency);
            }
            if let Some(ref adjclose) = stock.indicators.adjclose {
                if adjclose[0].adjclose.len() != n {
                    return Err(YahooError::DataInconsistency);
                }
            }
        }
        Ok(())
    }

    pub fn from_json(json: serde_json::Value) -> Result<YResponse, YahooError> {
        Ok(serde_json::from_value(json)?)
    }

    pub fn quotes(&self) -> Result<Vec<Quote>, YahooError> {
        self.check_consistency()?;
        let stock: &YQuoteBlock = &self.chart.result[0];
        let mut quotes = Vec::new();
        let n = stock.timestamp.len();
        for i in 0..n {
            let timestamp = stock.timestamp[i];
            let quote = stock.indicators.get_ith_quote(timestamp, i);
            if let Ok(q) = quote {
                quotes.push(q);
            }
        }
        Ok(quotes)
    }

    pub fn metadata(&self) -> Result<YMetaData, YahooError> {
        self.check_consistency()?;
        let stock = &self.chart.result[0];
        Ok(stock.meta.to_owned())
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct Quote {
    pub timestamp: u64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub volume: u64,
    pub close: f64,
    pub adjclose: f64,
}

#[derive(Deserialize, Debug)]
pub struct YChart {
    pub result: Vec<YQuoteBlock>,
    pub error: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct YQuoteBlock {
    pub meta: YMetaData,
    pub timestamp: Vec<u64>,
    pub indicators: QuoteBlock,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct YMetaData {
    pub currency: String,
    pub symbol: String,
    pub exchange_name: String,
    pub instrument_type: String,
}

#[derive(Deserialize, Debug)]
pub struct QuoteBlock {
    quote: Vec<QuoteList>,
    #[serde(default)]
    adjclose: Option<Vec<AdjClose>>,
}

impl QuoteBlock {
    fn get_ith_quote(&self, timestamp: u64, i: usize) -> Result<Quote, YahooError> {
        let adjclose = match &self.adjclose {
            Some(adjclose) => adjclose[0].adjclose[i],
            None => None,
        };
        let quote = &self.quote[0];
        // reject if close is not set
        if quote.close[i].is_none() {
            return Err(YahooError::EmptyDataSet);
        }
        Ok(Quote {
            timestamp,
            open: quote.open[i].unwrap_or(0.0),
            high: quote.high[i].unwrap_or(0.0),
            low: quote.low[i].unwrap_or(0.0),
            volume: quote.volume[i].unwrap_or(0),
            close: quote.close[i].unwrap(),
            adjclose: adjclose.unwrap_or(0.0),
        })
    }
}

#[derive(Deserialize, Debug)]
pub struct AdjClose {
    adjclose: Vec<Option<f64>>,
}

#[derive(Deserialize, Debug)]
pub struct QuoteList {
    pub volume: Vec<Option<u64>>,
    pub high: Vec<Option<f64>>,
    pub close: Vec<Option<f64>>,
    pub low: Vec<Option<f64>>,
    pub open: Vec<Option<f64>>,
}

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
    // sends the petition to yahoo, a fairly common user agent is necessary because otherwise we get rate limited
    let response = Client::new()
        .get(&format!("https://query1.finance.yahoo.com/v8/finance/chart/{ticker}?symbol={ticker}&period1={start}&period2={end}&interval=1d&events=div%7Csplit%7CcapitalGains"))
        .header("USER-AGENT", "curl/7.68.0")
        .send()
        .await
        ?;
    // serializes it and returns it
    Ok(YResponse::from_json(
        if let Ok(s) = serde_json::from_str(&response.text().await?) {
            s
        } else {
            return Err(ProviderError::YahooError);
        },
    )?)
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
