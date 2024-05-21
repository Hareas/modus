use actix_web::{App, get, HttpResponse, HttpServer, Responder, web};
use serde_json::json;
use modus::stock_returns::{Portfolio, StocksError, total_returns};
use modus::options::{Options, bs_price, kelly_ratio, expected};

#[get("/")]
async fn hello() -> impl Responder {
    HttpResponse::Ok().body("Hello world!")
}

async fn returns(item: web::Json<Portfolio>) -> impl Responder {
    match total_returns(item).await {
        Ok(res) => HttpResponse::Ok().json(res),
        Err(e) => match e {
            StocksError::ComponentRange => { HttpResponse::BadRequest().json(json!({"Error": "Failed to convert the date"})) }
            StocksError::YahooError => { HttpResponse::InternalServerError().json(json!({"Error": "Yahoo provided a wrong response or didn't respond"})) }
        }
    }
}

async fn bs (item: web::Json<Options>) -> impl Responder {
    HttpResponse::Ok().json(json!({"Price": bs_price(&item)}))
}

async fn kelly (item: web::Json<Options>) -> impl Responder {
    HttpResponse::Ok().json(json!({"Kelly fraction": kelly_ratio(&item)}))
}

async fn montecarlo (item: web::Json<Options>) -> impl Responder {
    match expected(&item) {
        Ok(res) => HttpResponse::Ok().json(json!({"Monte-Carlo value based on 10000 simulations": res})),
        Err(_) => HttpResponse::Ok().json(json!({"Error": "Some iterations couldn't be completed"}))
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .service(hello)
            .service(web::scope("/equities").route("/returns", web::get().to(returns)))
            .service(web::scope("/options")
                .route("/bs", web::get().to(bs))
                .route("/kelly", web::get().to(kelly))
                .route("/mc", web::get().to(montecarlo)))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
