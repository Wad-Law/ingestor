use anyhow::Result;

#[async_trait::async_trait]
pub trait Actor: Send + Sync + 'static {
    async fn run(self) -> Result<()>;
}

// ----------- Domain messages -----------------
#[derive(Clone, Debug)]
pub struct RawNews {
    pub content_hash: String,
    pub source: String,
    pub url: String,
    pub title: String,
    pub lede: String,
    pub ts_ms: i64,
    pub lang: String,
}

#[derive(Clone, Debug)]
pub struct PredictionMarket {
    pub market_id: String,
    pub slug: String,
    pub question: String,
    pub category: String,
    pub end_time_ms: i64,
    pub liquidity: f64,
    pub is_open: bool,
}

#[derive(Clone, Debug)]
pub struct MarketDataRequest{
    pub market_id: String
}

#[derive(Clone, Debug)]
pub struct MarketDataSnap {
    pub market_id: String,
    pub book_ts_ms: i64,
    pub best_bid: f32,
    pub best_ask: f32,
    pub bid_size: f32,
    pub ask_size: f32,
}

#[derive(Clone, Debug)]
pub struct Order {
    pub client_order_id: String,
    pub market_id: String,
    pub price: f32,
    pub size: f32,
}

#[derive(Clone, Debug)]
pub struct Execution {
    pub client_order_id: String,
    pub market_id: String,
    pub avg_px: f32,
    pub filled: f32,
    pub fee: f32,
    pub ts_ms: i64,
}