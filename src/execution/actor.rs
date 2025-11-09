use anyhow::Result;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use crate::bus::types::Bus;
use crate::core::types::{Actor};
use crate::core::types::Execution;

pub struct ExecutionActor {
    pub bus: Bus,
    pub shutdown: CancellationToken
}

impl ExecutionActor {
    pub fn new(bus: Bus, shutdown: CancellationToken) -> ExecutionActor {
        Self { bus, shutdown }
    }
}

#[async_trait::async_trait]
impl Actor for ExecutionActor {
    async fn run(mut self) -> Result<()> {

        info!("ExecutionActor started");
        let mut rx = self.bus.orders.subscribe(); // broadcast::Receiver<Arc<MarketDataRequest>>
        loop {
            tokio::select! {
                // Graceful shutdown signal
                _ = self.shutdown.cancelled() => {
                        info!("StrategyActor: shutdown requested");
                        break;
                }

                // Order requests
                res = rx.recv() => {
                    match res {
                        Ok(req) => {
                            // TODO: send order

                            // TODO:: GET FILL
                            let fill = Execution {
                                client_order_id: "".to_string(),
                                market_id: "".to_string(),
                                avg_px: 0.0,
                                filled: 0.0,
                                fee: 0.0,
                                ts_ms: 0,
                            };

                            self.bus.executions.publish(fill).await?;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            // a slow consumer skipped n messages
                            error!("ExecutionActor lagged by {n} MarketDataRequest messages");
                            continue;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            // no more senders; decide whether to exit
                            error!("ExecutionActor request channel closed");
                            break;
                        }
                    }
                }
            }
        }

        info!("ExecutionActor stopped cleanly");
        Ok(())
    }
}