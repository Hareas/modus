use actix_web::{get, post, web, App, HttpResponse, HttpServer, Responder};
use yahoo_finance_api as yahoo;
use time::{macros::datetime};

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

async fn index() -> impl Responder {
    let provider = yahoo::YahooConnector::new();
    let start = datetime!(2020-1-1 0:00:00.00 UTC);
    let end = datetime!(2020-1-31 23:59:59.99 UTC);
    // returns historic quotes with daily interval
    match provider.get_quote_history("AAPL", start, end).await {
        Ok(resp) => {
            let quotes = resp.quotes().unwrap();
            println!("Apple's quotes in January: {:?}", quotes);
            HttpResponse::Ok().body("Quotes fetched successfully")
        }
        Err(err) => {
            eprintln!("Error fetching quotes: {:?}", err);
            HttpResponse::InternalServerError().body("Error fetching quotes")
        }
    }
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
