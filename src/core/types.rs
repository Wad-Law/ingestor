use anyhow::Result;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[async_trait::async_trait]
pub trait Actor: Send + Sync + 'static {
    async fn run(self) -> Result<()>;
}

// ----------- Domain messages -----------------
#[derive(Clone, Debug)]
pub struct RawNews {
    #[allow(dead_code)]
    pub url: String,
    pub title: String,
    pub description: String,
    #[allow(dead_code)]
    pub feed: String,
    #[allow(dead_code)]
    pub published: Option<chrono::DateTime<chrono::Utc>>,
    #[allow(dead_code)]
    pub labels: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct MarketDataRequest {
    pub market_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketToken {
    pub token_id: String,
    pub outcome: String, // "Yes", "No"
    pub price: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketDataSnap {
    pub market_id: String,
    pub book_ts_ms: i64,
    pub best_bid: Decimal,
    pub best_ask: Decimal,
    pub bid_size: Decimal,
    pub ask_size: Decimal,
    #[serde(default)]
    pub tokens: Option<Vec<MarketToken>>,
    #[serde(default)]
    pub question: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub enum Side {
    Buy,
    #[allow(dead_code)]
    Sell,
}

#[derive(Clone, Debug)]
pub struct Order {
    pub client_order_id: String,
    pub market_id: String,
    pub token_id: Option<String>, // Specific token to trade
    pub side: Side,
    pub price: Decimal,
    pub size: Decimal,
}

#[derive(Clone, Debug)]
pub struct Execution {
    #[allow(dead_code)]
    pub client_order_id: String,
    #[allow(dead_code)]
    pub market_id: String,
    #[allow(dead_code)]
    pub avg_px: Decimal,
    #[allow(dead_code)]
    pub filled: Decimal,
    #[allow(dead_code)]
    pub fee: Decimal,
    #[allow(dead_code)]
    pub ts_ms: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PolyMarketEvent {
    #[allow(dead_code)]
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub markets: Option<Vec<PolyMarketMarket>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PolyMarketMarket {
    pub id: String,
    #[serde(default)]
    pub question: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}
