use crate::bus::types::Bus;
use crate::core::types::{Actor, RawNews};
use anyhow::Result;
use tokio_util::sync::CancellationToken;
use tracing::{info};

pub struct AltDataActor {
    pub bus: Bus,
    pub shutdown: CancellationToken
}

impl AltDataActor {
    pub fn new(bus: Bus, shutdown: CancellationToken) -> AltDataActor {
        Self { bus, shutdown }
    }

    async fn fetch_once(&self) -> Result<()> {
        // TODO: replace with your real fetch
        // e.g., let body = self.client.get(&self.url).send().await?.bytes().await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl Actor for AltDataActor {
    async fn run(mut self) -> Result<()> {
        info!("AltDataActor started");
        loop {
            tokio::select! {
                // Graceful shutdown signal
                _ = self.shutdown.cancelled() => {
                    info!("StrategyActor: shutdown requested");
                    break; // exit loop -> return Ok(())
                }
                
                // Fetch news
                _ = self.fetch_once() => {
                     //TODO fetch news
                    let raw_news = RawNews{
                        content_hash: "".to_string(),
                        source: "".to_string(),
                        url: "".to_string(),
                        title: "".to_string(),
                        lede: "".to_string(),
                        ts_ms: 0,
                        lang: "".to_string(),
                    };
                    self.bus.raw_news.publish(raw_news).await?;
                }
            }
        }
        info!("AltDataActor stopped cleanly");
        Ok(())
    }
}