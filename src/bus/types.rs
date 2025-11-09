use std::sync::Arc;
use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::{broadcast};
use crate::core::types::{RawNews, PredictionMarket, MarketDataRequest, MarketDataSnap, Order, Execution};

// ---------- Topic trait (broadcast semantics) ----------
#[async_trait::async_trait]
pub trait Topic<T>: Sync + Send + 'static {
    /// Publish a message to all subscribers.
    async fn publish(&self, msg: T) -> Result<()>;

    /// Subscribe to the stream (each subscriber has an independent cursor).
    fn subscribe(&self) -> broadcast::Receiver<Arc<T>>;
}

// ---------- Concrete broadcast topic ----------
// --- Broadcast topic: 1->N fanout (lossy under lag). Wrap payloads in Arc<T> to avoid Clone on T.
pub struct BroadcastTopic<T: Clone + Send + Sync + 'static> {
    tx: broadcast::Sender<Arc<T>>,
}

impl<T: Clone + Send + Sync + 'static> BroadcastTopic<T> {
    pub fn with_capacity(cap: usize) -> Self {
        let (tx, _rx) = broadcast::channel(cap);
        Self { tx }
    }
}

#[async_trait]
impl<T: Clone + Send + Sync + 'static> Topic<T> for BroadcastTopic<T> {
    async fn publish(&self, msg: T) -> Result<()> {
        // Non-blocking; errors only when no receivers (we can ignore or log)
        let _ = self.tx.send(Arc::new(msg));
        Ok(())
    }

    fn subscribe(&self) -> broadcast::Receiver<Arc<T>> {
        self.tx.subscribe()
    }
}

#[derive(Clone)]
pub struct Bus {
    pub raw_news: Arc<dyn Topic<RawNews>>,
    pub prediction_market: Arc<dyn Topic<PredictionMarket>>,
    pub market_data_request: Arc<dyn Topic<MarketDataRequest>>,
    pub market_data: Arc<dyn Topic<MarketDataSnap>>,
    pub orders: Arc<dyn Topic<Order>>,
    pub executions: Arc<dyn Topic<Execution>>,
}

impl Bus {
    pub fn new() -> Self {
        let cap = 1024;

        Self {
            raw_news: Arc::new(BroadcastTopic::<RawNews>::with_capacity(cap)),
            prediction_market: Arc::new(BroadcastTopic::<PredictionMarket>::with_capacity(cap)),
            market_data_request: Arc::new(BroadcastTopic::<MarketDataRequest>::with_capacity(cap)),
            market_data: Arc::new(BroadcastTopic::<MarketDataSnap>::with_capacity(cap)),
            orders: Arc::new(BroadcastTopic::<Order>::with_capacity(cap)),
            executions: Arc::new(BroadcastTopic::<Execution>::with_capacity(cap)),
        }
    }
}