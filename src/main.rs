use std::collections::BTreeMap;

use actix_web::{App, get, HttpResponse, HttpServer, post, Responder, web};
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use time::{Date, Month, OffsetDateTime};
use time::macros::time;

use modus::yahoo_finance::{get_quotes, handle_response};

#[derive(Debug, Serialize, Deserialize)]
struct Position {
    price: f64,
    quantity: u32
}

#[derive(Debug, Serialize, Deserialize)]
struct Portfolio {
    portfolio: Vec<Equity>
}

#[derive(Debug, Serialize, Deserialize)]
struct Equity {
    ticker: String,
    buy: Transaction,
    sell: Option<Transaction>,
    quantity: u32
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
    day: u8
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
            _ => Month::January
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
            Date::from_calendar_date(
                n.buy.date.year,
                n.buy.date.match_month(),
                n.buy.date.day,
            )
                .unwrap(),
            time!(0:00:00),
        );
        let end = match &n.sell {
            Some(sell) => OffsetDateTime::new_utc(
                Date::from_calendar_date(
                    sell.date.year,
                    sell.date.match_month(),
                    sell.date.day,
                )
                    .unwrap(),
                time!(23:59:59),
            ),
            None => OffsetDateTime::now_utc(),
        };
        for m in get_quotes(&n.ticker, &start, &end).await.unwrap().iter() {
            returns
                .entry(DateTime::from_timestamp(m.timestamp as i64, 0).unwrap().date_naive())
                .or_insert_with(Vec::new)
                .push(Position {
                    price: m.adjclose,
                    quantity: n.quantity,
                });
/*            match returns.get_mut(&m.timestamp) {
                Some(pos) => pos.push(Position{price: m.adjclose, quantity: n.quantity}),
                None => returns.insert(m.timestamp, vec![Position{price: m.adjclose, quantity: n.quantity}])
            }
            returns.insert(m.timestamp, vec![].push(Position{price: m.adjclose, quantity: n.quantity}));
            returns.push(Return {timestamp: m.timestamp , position: vec![Position{price: m.adjclose, quantity: n.quantity}]});*/
        }
/*        quotes.push(get_quotes(&n.ticker, &start, &end).await.unwrap());*/
    }
    println!("model: {:?}", &returns);
    handle_response().await
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .service(hello)
            .service(echo)
            .route("/hey", web::get().to(manual_hello))
            .service(
                web::scope("/equities")
                    .route("/index", web::get().to(index)),
            )
    })
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}
