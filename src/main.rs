use std::collections::BTreeMap;

use actix_web::{get, post, web, App, HttpResponse, HttpServer, Responder};
use chrono::{DateTime, NaiveDate};
use serde::{Deserialize, Serialize};
use time::macros::time;
use time::{Date, Month, OffsetDateTime};

use modus::yahoo_finance::{get_quotes, handle_response};

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

#[get("/")]
async fn hello() -> impl Responder {
    HttpResponse::Ok().body("Hello world!")
}

#[post("/echo")]
async fn echo(req_body: String) -> impl Responder {
    HttpResponse::Ok().body(req_body)
}

async fn manual_hello() -> impl Responder {
    HttpResponse::Ok().body("Hey there!")
}

async fn index(item: web::Json<Portfolio>) -> impl Responder {
    println!("model: {:?}", &item);
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
    println!("model: {:?}", &total_returns);
    HttpResponse::Ok().json(total_returns)
    //handle_response().await
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .service(hello)
            .service(echo)
            .route("/hey", web::get().to(manual_hello))
            .service(web::scope("/equities").route("/index", web::get().to(index)))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
