use std::collections::BTreeMap;
use std::sync::{Arc, mpsc};
use std::thread;
use actix_web::{App, get, HttpResponse, HttpServer, post, Responder, web};
use chrono::DateTime;
use rstat::Distribution;
use rstat::univariate::normal::Normal;
use serde::{Deserialize, Serialize};
use time::{Date, Month, OffsetDateTime};
use time::macros::time;
use serde_json::json;

use modus::yahoo_finance::get_quotes;

#[derive(Debug, Serialize, Deserialize)]
struct Position {
    old_price: f64,
    price: f64,
    quantity: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct Portfolio {
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

#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
struct Options {
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

#[get("/")]
async fn hello() -> impl Responder {
    HttpResponse::Ok().body("Hello world!")
}

async fn index(item: web::Json<Portfolio>) -> impl Responder {
    let mut returns = BTreeMap::new();
    for n in item.portfolio.iter() {
        let start = OffsetDateTime::new_utc(
            Date::from_calendar_date(n.buy.date.year, n.buy.date.match_month(), n.buy.date.day)
                .unwrap(),
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
        let quotes = get_quotes(&n.ticker, &start, &end).await.unwrap();
        for (i, m) in quotes.iter().enumerate() {
            returns
                .entry(
                    DateTime::from_timestamp(m.timestamp as i64, 0)
                        .unwrap()
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
    let total_returns :BTreeMap<String, f64> = returns
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
        .collect();
    HttpResponse::Ok().json(total_returns)
}

async fn bs (item: web::Json<Options>) -> impl Responder {
    HttpResponse::Ok().json(json!({"Price": bs_price(&item)}))
}

fn bs_price (item: &Options) -> f64 {
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

async fn kelly (item: web::Json<Options>) -> impl Responder {
    HttpResponse::Ok().json(json!({"Kelly fraction": kelly_ratio(&item)}))
}

fn kelly_ratio (item: &Options) -> f64 {
    let d1 = d1(item);
    let d2 = d2(d1, item);
    let w = (bs_price(item) / Normal::standard().cdf(&d2) - item.market_price.unwrap()) / item.market_price.unwrap();
    (Normal::standard().cdf(&d2) * w - (1.0 - Normal::standard().cdf(&d2))) / w
}

async fn montecarlo (item: web::Json<Options>) -> impl Responder {
    let values = Arc::new(*item);
    let (tx, rx) = mpsc::channel();
    for _ in 0..10000 {
        let (values, tx) = (values.clone(), tx.clone());
        thread::spawn(move || {
            let data = values.underlying * ((values.rfr - values.volatility.powi(2) / 2.0) * values.maturity as f64 + values.volatility * (values.maturity as f64).sqrt() * Normal::standard().sample(&mut rand::thread_rng())).exp();
            tx.send(data).unwrap()
        });
    }
    let mut v :Vec<f64> = Vec::new();
    for _ in 0..10000 {
        v.push(rx.recv().unwrap());
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
    let expected = returns.iter().sum::<f64>() / returns.len() as f64;
    HttpResponse::Ok().json(json!({"Monte-Carlo value": expected}))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .service(hello)
            .service(web::scope("/equities").route("/index", web::get().to(index)))
            .service(web::scope("/options")
                .route("/bs", web::get().to(bs))
                .route("/kelly", web::get().to(kelly))
                .route("/mc", web::get().to(montecarlo)))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
