use std::net::Shutdown;
use crate::bus::types::Bus;
use crate::core::types::{Actor, Execution, MarketDataSnap, Order, RawNews};
use anyhow::Result;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

pub struct StrategyActor {
    pub bus: Bus,
    pub shutdown: CancellationToken
}

impl StrategyActor {
    pub fn new(bus: Bus, shutdown: CancellationToken) -> StrategyActor {
        Self { bus, shutdown }
    }

    fn decide_from_tick(&self, snap: &MarketDataSnap) -> Option<Order> {
        // TODO: real logic
        None
    }

    fn decide_from_news(&self, news: &RawNews) -> Option<Order> {
        // TODO: real logic
        None
    }

    fn decide_from_executions(&self, news: &Execution) -> Option<Order> {
        // TODO: real logic
        None
    }
}

#[async_trait::async_trait]
impl Actor for StrategyActor {
    async fn run(mut self) -> Result<()> {
        info!("StrategyActor started");

        // Subscribe to both broadcast streams
        let mut md_rx   = self.bus.market_data.subscribe(); // broadcast::Receiver<Arc<MarketDataSnap>>
        let mut news_rx = self.bus.raw_news.subscribe();     // broadcast::Receiver<Arc<RawNews>>
        let mut executions_rx = self.bus.executions.subscribe();     // broadcast::Receiver<Arc<Executions>>

        loop {
            tokio::select! {
                // Graceful shutdown signal
                _ = self.shutdown.cancelled() => {
                    info!("StrategyActor: shutdown requested");
                    break;
                }

                // Market data path
                res = md_rx.recv() => {
                    match res {
                        Ok(snap) => {
                            if let Some(order) = self.decide_from_tick(&snap) {
                                // Publish order to orders topic
                                self.bus.orders.publish(order).await?;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            warn!(lagged = n, "StrategyActor lagged on market_data");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            error!("market_data stream closed; exiting StrategyActor");
                            break;
                        }
                    }
                }

                // News path
                res = news_rx.recv() => {
                    match res {
                        Ok(news) => {
                            if let Some(order) = self.decide_from_news(&news) {
                                self.bus.orders.publish(order).await?;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            warn!(lagged = n, "StrategyActor lagged on raw_news");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            error!("raw_news stream closed; exiting StrategyActor");
                            break;
                        }
                    }
                }

                // executions path
                res = executions_rx.recv() => {
                    match res {
                        Ok(executions) => {
                            if let Some(order) = self.decide_from_executions(&executions) {
                                self.bus.orders.publish(order).await?;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            warn!(lagged = n, "StrategyActor lagged on executions");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            error!("executions stream closed; exiting StrategyActor");
                            break;
                        }
                    }
                }
            }
        }
        info!("StrategyActor stopped cleanly");
        Ok(())
    }
}