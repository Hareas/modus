use std::sync::{Arc, mpsc};
use std::thread;
use actix_web::{App, get, HttpResponse, HttpServer, Responder, web};
use rstat::Distribution;
use rstat::univariate::normal::Normal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use modus::stock_returns::{Portfolio, total_returns};

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

async fn returns(item: web::Json<Portfolio>) -> impl Responder {
    HttpResponse::Ok().json(total_returns(item).await)
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
