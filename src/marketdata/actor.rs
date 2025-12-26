use crate::bus::types::Bus;
use crate::config::config::PolyCfg;
use crate::core::types::Actor;
use crate::core::types::MarketDataSnap;
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use rust_decimal::Decimal;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

#[derive(Debug, Deserialize)]
struct PolyToken {
    token_id: String,
    outcome: String,
    price: Decimal,
}

#[derive(Debug, Deserialize)]
struct PolyMarketResponse {
    id: String,
    tokens: Option<Vec<PolyToken>>,
    best_bid: Option<Decimal>,
    best_ask: Option<Decimal>,
    question: String,
}

pub struct MarketPricingActor {
    pub bus: Bus,
    pub client: Client,
    pub poly_cfg: PolyCfg,
    pub shutdown: CancellationToken,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct PolyMarketDetail {
    // id: String,
    // question: String,
    #[serde(default)]
    best_bid: Option<Decimal>,
    #[serde(default)]
    best_ask: Option<Decimal>,
    // spread: Option<f64>,
}

impl MarketPricingActor {
    pub fn new(
        bus: Bus,
        client: Client,
        poly_cfg: PolyCfg,
        shutdown: CancellationToken,
    ) -> MarketPricingActor {
        Self {
            bus,
            client,
            poly_cfg,
            shutdown,
        }
    }

    fn get_market_url(&self, id: &str) -> String {
        format!("{}/{}", self.poly_cfg.gamma_markets_url, id)
    }

    async fn fetch_market_data(&self, market_id: &str) -> Result<MarketDataSnap> {
        let url = self.get_market_url(market_id);

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("requesting market data")?;

        if !resp.status().is_success() {
            anyhow::bail!("Gamma API error: {}", resp.status());
        }

        let poly_resp: PolyMarketResponse = resp.json().await.context("parsing market data")?;

        let tokens = poly_resp.tokens.map(|ts| {
            ts.into_iter()
                .map(|t| crate::core::types::MarketToken {
                    token_id: t.token_id,
                    outcome: t.outcome,
                    price: t.price,
                })
                .collect()
        });

        Ok(MarketDataSnap {
            market_id: poly_resp.id,
            book_ts_ms: chrono::Utc::now().timestamp_millis(), // Approximate
            best_bid: poly_resp.best_bid.unwrap_or(Decimal::ZERO),
            best_ask: poly_resp.best_ask.unwrap_or(Decimal::ZERO),
            bid_size: Decimal::ZERO, // Not provided in simple endpoint
            ask_size: Decimal::ZERO,
            tokens,
            question: poly_resp.question,
        })
    }
}

#[async_trait]
impl Actor for MarketPricingActor {
    async fn run(mut self) -> Result<()> {
        info!("MarketPricingActor started");
        let mut rx = self.bus.market_data_request.subscribe();
        loop {
            tokio::select! {
                // Graceful shutdown signal
                _ = self.shutdown.cancelled() => {
                    info!("MarketDataActor: shutdown requested");
                    break;
                }

                // market data requests
                res = rx.recv() => {
                    match res {
                        Ok(req) => {
                            match self.fetch_market_data(&req.market_id).await {
                                Ok(snap) => {
                                    if let Err(e) = self.bus.market_data.publish(snap).await {
                                        error!("Failed to publish market data: {}", e);
                                    }
                                }
                                Err(e) => {
                                    warn!("Failed to fetch market data for {}: {}", req.market_id, e);
                                }
                            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::types::Bus;
    use crate::config::config::PolyCfg;
    use std::time::Duration;

    fn mock_poly_cfg() -> PolyCfg {
        PolyCfg {
            base_url: "https://clob.polymarket.com".to_string(),
            gamma_events_url: "http://localhost/events".to_string(),
            gamma_markets_url: "http://localhost/markets".to_string(),
            market_list_refresh: Duration::from_secs(1),
            page_limit: 10,
            ascending: false,
            include_closed: false,
            api_key: "".to_string(),
            api_secret: "".to_string(),
            passphrase: "".to_string(),
            token_decimals: 6,
            rpc_url: "http://localhost:8545".to_string(),
            data_api_url: "http://localhost/positions".to_string(),
        }
    }

    #[tokio::test]
    async fn test_market_data_actor_flow() {
        let bus = Bus::new();
        let client = Client::new();
        let cfg = mock_poly_cfg();
        let shutdown = CancellationToken::new();

        let actor = MarketPricingActor::new(bus, client, cfg, shutdown);
        assert_eq!(actor.poly_cfg.gamma_markets_url, "http://localhost/markets");
    }

    #[tokio::test]
    async fn test_url_construction() {
        let cfg = mock_poly_cfg();
        let market_id = "12345";
        let url = format!("{}/{}", cfg.gamma_markets_url, market_id);
        assert_eq!(url, "http://localhost/markets/12345");
    }
}
