use actix_web::{get, post, web, App, HttpResponse, HttpServer, Responder};
use time::macros::{time};
use modus::yahoo_finance::{get_quotes, handle_response};
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, Month};
use yahoo_finance_api::Quote;

#[derive(Debug, Serialize, Deserialize)]
struct Portfolio {
    portfolio: Vec<Equity>
}

#[derive(Debug, Serialize, Deserialize)]
struct Equity {
    ticker: String,
    buy: Transaction,
    sell: Option<Transaction>,
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
    let quotes :Vec<Quote>;
    for n in item.portfolio.iter() {
        let start = OffsetDateTime::now_utc()
            .replace_year(n.buy.date.year).unwrap()
            .replace_month(n.buy.date.match_month()).unwrap()
            .replace_day(n.buy.date.day).unwrap()
            .replace_time(time!(0:00:00));
        let end = match &n.sell {
            Some(_) => {
                OffsetDateTime::now_utc()
                    .replace_year(n.sell.as_ref().unwrap().date.year).unwrap()
                    .replace_month(n.sell.as_ref().unwrap().date.match_month()).unwrap()
                    .replace_day(n.sell.as_ref().unwrap().date.day).unwrap()
                    .replace_time(time!(23:59:59))
            },
            _ => OffsetDateTime::now_utc()
        };
        get_quotes(&n.ticker, &start, &end).await;
    }
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
