use actix_web::{get, post, web, App, HttpResponse, HttpServer, Responder};
use time::macros::datetime;
use modus::yahoo_finance::{get_quotes};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Portfolio {
    portfolio: Vec<Equity>
}

#[derive(Debug, Serialize, Deserialize)]
struct Equity {
    ticker: String,
    buy_date: String,
    sell_date: Option<String>,
    buy_price: u64,
    sell_price: Option<u64>,
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
    let start = datetime!(2020-1-1 0:00:00.00 UTC);
    let end = datetime!(2020-1-31 23:59:59.99 UTC);
    get_quotes("AAPL", &start, &end).await
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
