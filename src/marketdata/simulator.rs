use crate::core::types::MarketDataSnap;
use crate::marketdata::client::MarketDataClient;
use anyhow::Result;
use async_trait::async_trait;
use rust_decimal::Decimal;

pub struct SimMarketDataClient;

impl SimMarketDataClient {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl MarketDataClient for SimMarketDataClient {
    async fn fetch_market_data(&self, market_id: &str) -> Result<MarketDataSnap> {
        // Return dummy data or random walk
        Ok(MarketDataSnap {
            market_id: market_id.to_string(),
            book_ts_ms: chrono::Utc::now().timestamp_millis(),
            best_bid: Decimal::new(50, 2), // 0.50
            best_ask: Decimal::new(51, 2), // 0.51
            bid_size: Decimal::new(1000, 0),
            ask_size: Decimal::new(1000, 0),
            tokens: Some(vec![
                crate::core::types::MarketToken {
                    token_id: format!("{}-Yes", market_id),
                    outcome: "Yes".to_string(),
                    price: Decimal::new(50, 2),
                },
                crate::core::types::MarketToken {
                    token_id: format!("{}-No", market_id),
                    outcome: "No".to_string(),
                    price: Decimal::new(50, 2),
                },
            ]),
            question: "Simulated Market".to_string(),
        })
    }
}
