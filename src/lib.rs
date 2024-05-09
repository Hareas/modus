pub mod yahoo_finance {
    use actix_web::{HttpResponse, Responder};
    use yahoo_finance_api as yahoo;
    use time::macros::datetime;

    pub async fn yahoo_it () -> impl Responder{
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
}