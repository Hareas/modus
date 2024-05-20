pub mod yahoo_finance {
    use actix_web::{HttpResponse, Responder};
    use time::OffsetDateTime;
    use yahoo_finance_api as yahoo;
    use yahoo_finance_api::{Quote, YahooError};

    async fn yahoo_it (ticker: &str, start: &OffsetDateTime, end: &OffsetDateTime) -> Result<Vec<Quote>, YahooError> {
        let provider = yahoo::YahooConnector::new();
        // returns historic quotes with daily interval
        provider.get_quote_history(ticker, *start, *end).await?.quotes()
    }
    
    pub async fn get_quotes (ticker: &str, start: &OffsetDateTime, end: &OffsetDateTime) -> Result<Vec<Quote>, YahooError> {
        yahoo_it(ticker, start, end).await
    }

    pub async fn handle_response () -> impl Responder {
        HttpResponse::Ok().body("Hey there!")
    }
}