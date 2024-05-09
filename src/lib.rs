pub mod yahoo_finance {
    use actix_web::{HttpResponse, Responder};
    use yahoo_finance_api as yahoo;
    use time::OffsetDateTime;

    async fn yahoo_it (ticker: &str, start: &OffsetDateTime, end: &OffsetDateTime) -> impl Responder {
        let provider = yahoo::YahooConnector::new();
        // returns historic quotes with daily interval
        match provider.get_quote_history(ticker, *start, *end).await {
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
    
    pub async fn get_quotes (ticker: &str, start: &OffsetDateTime, end: &OffsetDateTime) -> impl Responder {
        yahoo_it(ticker, start, end).await
    }
}