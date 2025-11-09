use anyhow::Result;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use crate::bus::types::Bus;
use crate::core::types::Actor;
use crate::core::types::MarketDataSnap;

pub struct MarketDataActor {
    pub bus: Bus,
    pub shutdown: CancellationToken
}

impl MarketDataActor {
    pub fn new(bus: Bus, shutdown: CancellationToken) -> MarketDataActor {
        Self { bus, shutdown }
    }
}

#[async_trait::async_trait]
impl Actor for MarketDataActor {
    async fn run(mut self) -> Result<()> {
        info!("MarketDataActor started");
        let mut rx = self.bus.market_data_request.subscribe(); // broadcast::Receiver<Arc<MarketDataRequest>>
        loop {
            tokio::select! {
                // Graceful shutdown signal
                _ = self.shutdown.cancelled() => {
                    info!("StrategyActor: shutdown requested");
                    break;
                }

                // market data requests
                res = rx.recv() =>
                {
                    match res {
                        Ok(req) => {
                            // TODO: fetch data using *req Arc<MarketDataRequest>
                            let snap = MarketDataSnap {
                                market_id: "".to_string(),
                                book_ts_ms: 0,
                                best_bid: 0.0,
                                best_ask: 0.0,
                                bid_size: 0.0,
                                ask_size: 0.0,
                            };
                            self.bus.market_data.publish(snap).await?;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            // a slow consumer skipped n messages
                            error!("MarketDataActor lagged by {n} MarketDataRequest messages");
                            continue;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            // no more senders; decide whether to exit
                            error!("MarketDataActor request channel closed");
                            break;
                        }
                    }
                }
            }
        }

        info!("MarketDataActor stopped cleanly");
        Ok(())
    }
}