use crate::bus::types::Bus;
use crate::config::config::AppCfg;
use crate::core::types::{
    Actor, Execution, MarketDataRequest, MarketDataSnap, Order, PolyMarketEvent, RawNews,
};
use crate::llm::LlmClient;
use crate::strategy::canonical_event::CanonicalEventBuilder;
use crate::strategy::event_features::{EventFeatureExtractor, FeatureDictionaries};
use crate::strategy::exact_duplicate_detector::{
    ExactDuplicateDetector, ExactDuplicateDetectorConfig,
};
use crate::strategy::hard_filters::HardFilterer;
use crate::strategy::kelly::KellySizer;
use crate::strategy::market_index::MarketIndex;
use crate::strategy::sim_hash_cache::{SimHashCache, SimHashCacheConfig};
use crate::strategy::tokenization::{TokenizationConfig, TokenizedNews};
use crate::strategy::types::*;
use anyhow::Result;
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

pub struct StrategyActor {
    pub bus: Bus,
    pub shutdown: CancellationToken,
    pub detector: ExactDuplicateDetector,
    pub sim_hash_cache: SimHashCache,
    pub event_feature_extractor: EventFeatureExtractor,
    pub canonical_builder: CanonicalEventBuilder,
    pub market_index: MarketIndex,
    pub hard_filterer: HardFilterer,
    pub kelly_sizer: KellySizer,
    pub market_data_cache: HashMap<String, MarketDataSnap>,
    pub bankroll: Decimal,
    pub llm_client: LlmClient,
}

impl StrategyActor {
    pub fn new(bus: Bus, shutdown: CancellationToken, cfg: &AppCfg) -> StrategyActor {
        Self {
            bus,
            shutdown,
            detector: ExactDuplicateDetector::new(ExactDuplicateDetectorConfig::default()),
            sim_hash_cache: SimHashCache::new(SimHashCacheConfig::default()),
            event_feature_extractor: EventFeatureExtractor::new(
                FeatureDictionaries::default_minimal(),
            ),
            canonical_builder: CanonicalEventBuilder::new(),
            market_index: MarketIndex::new().expect("Failed to initialize MarketIndex"),
            hard_filterer: HardFilterer::new(),
            kelly_sizer: KellySizer::default(),
            market_data_cache: HashMap::new(),
            bankroll: Decimal::from_f64(cfg.strategy.bankroll).unwrap_or(Decimal::ZERO),
            llm_client: LlmClient::new(cfg.llm.clone()),
        }
    }

    fn decide_from_tick(&mut self, snap: &MarketDataSnap) -> Option<Order> {
        self.market_data_cache
            .insert(snap.market_id.clone(), snap.clone());
        // High-level responsibilities:
        // - Update internal view of prices/PNL (later via Monitoring/Risk).
        // - Possibly adjust / rebalance positions:
        //   * Take profit if price converged to belief.
        //   * Cut risk if edge has flipped.
        //   * Manage time-based exits as resolution approaches.
        //
        // For now: delegate to a placeholder:
        None
    }

    // ============================
    // Pipeline: News / Events
    // ============================

    /// Core pipeline for a fresh news item:
    /// 1) Dedup
    /// 2) Normalize + tokenize
    /// 3) Entity & date extraction
    /// 4) Candidate generation (BM25)
    /// 5) Hard filters
    /// 6) Heuristic scoring
    /// 7) Top-K selection with diversity
    /// 8) Score -> probability (logistic)
    /// 9) Compare belief vs market price
    /// 10) Kelly sizing + risk caps
    /// 11) Build orders
    async fn handle_news_event(&mut self, raw_news: &RawNews) -> Vec<Order> {
        let order = Vec::new();
        // RawNews → normalized string → exact dedup
        //                ↓
        //            tokenize
        //                ↓
        //            SimHash → near-duplicate dedup
        //                ↓
        //      entity/date extraction → BM25 → scoring → decision

        // 1. Cheap exact dedup to eliminate trivial duplicates
        if self.detector.is_duplicate(raw_news) {
            println!("Skipping duplicate news.");
        } else {
            println!("New event — continue pipeline.");
        }

        // 2. Tokenize
        let cfg = TokenizationConfig::default();
        let tokenized_news = TokenizedNews::from_raw(raw_news.clone(), &cfg);

        // 3. Semantic dedup (SimHash) to eliminate rewritten versions
        let h = self.sim_hash_cache.sim_hash(&tokenized_news.tokens);
        if self.sim_hash_cache.is_near_duplicate(h) {
            info!("Near-duplicate news skipped (SimHash).");
            return order;
        }
        self.sim_hash_cache.insert(h);

        // 3. lexical and semantic using hard coded rules => semantic is generated from lexical
        let now = Utc::now();
        let feat = self.event_feature_extractor.extract(&tokenized_news, now); // Lexical layer (layers, numbers, time window)
        let _canonical = self.canonical_builder.build(&tokenized_news, &feat); // semantic layer (domain, event kind, primary/secondary entities, number interpretation, location, semantic time window)

        // 4. Candidate generation with Hybrid Search (BM25 + Semantic)
        let raw_candidates =
            self.retrieve_candidates(tokenized_news.tokens.as_slice(), &raw_news.title);

        // 5. Hard filters to kill false positives
        let entities = &feat.entities;
        let time_window = &feat.time_window;
        let filtered_candidates = self
            .hard_filterer
            .apply(raw_candidates, entities, time_window);

        // 6. LLM Scoring
        // 6. LLM Scoring
        let mut edged_candidates = Vec::new();

        // Take top N candidates for LLM analysis to save costs
        let top_candidates = filtered_candidates.into_iter().take(5).collect::<Vec<_>>();

        // Ensure we have market data (Price + Question) for these candidates
        self.ensure_market_data(&top_candidates).await;

        for candidate in top_candidates {
            // Retrieve market question/title from index or cache (simplified here using candidate data if available)
            // We need the market question. The candidate has market_id.
            // We can try to get it from market_data_cache if available, or we might need to store it in the index.
            // For now, let's assume we can get it from the cache or it's part of the candidate metadata (which it isn't yet).
            // Fallback: use a placeholder or skip if missing.

            // In a real implementation, we'd fetch the market title from a MarketStore.
            // Let's assume we have it in market_data_cache for now.
            let market_question =
                if let Some(snap) = self.market_data_cache.get(&candidate.market_id) {
                    snap.question.clone()
                } else {
                    "Unknown Market Question".to_string()
                };

            // Call LLM
            match self
                .llm_client
                .analyze(&raw_news.title, &market_question)
                .await
            {
                Ok(signal) => {
                    info!("LLM Signal for {}: {:?}", candidate.market_id, signal);

                    // Convert signal to TradeSide and Probability
                    let (_side, prob) = match signal.sentiment.as_str() {
                        "Positive" => (
                            TradeSide::BuyYes,
                            Decimal::from_f64(0.5 + (signal.confidence * 0.5))
                                .unwrap_or(Decimal::new(5, 1)),
                        ),
                        "Negative" => (
                            TradeSide::BuyNo,
                            Decimal::from_f64(0.5 + (signal.confidence * 0.5))
                                .unwrap_or(Decimal::new(5, 1)),
                        ),
                        _ => (TradeSide::BuyYes, Decimal::new(5, 1)), // Neutral 0.5
                    };

                    // Get market price
                    let market_price =
                        if let Some(snap) = self.market_data_cache.get(&candidate.market_id) {
                            (snap.best_bid + snap.best_ask) / Decimal::new(2, 0)
                        } else {
                            Decimal::new(5, 1) // 0.5
                        };

                    if prob > Decimal::new(6, 1) {
                        // Threshold 0.6
                        let edged = EdgedCandidate {
                            candidate,
                            score: prob,
                            probability: prob,
                            market_price,
                            edge: prob - market_price,
                        };
                        edged_candidates.push(edged);
                    }
                }
                Err(e) => {
                    error!("LLM analysis failed for {}: {:?}", candidate.market_id, e);
                }
            }
        }

        // 7. Kelly Sizing
        let sized_decisions = self.kelly_sizer.size_positions(edged_candidates);

        // 8. Build Orders
        self.build_orders_from_sized_decisions(&sized_decisions)
    }

    fn retrieve_candidates(&mut self, tokens: &[String], raw_text: &str) -> Vec<RawCandidate> {
        let mut candidates = HashMap::new();

        // 1. BM25 Search
        if let Ok(results) = self.market_index.search(tokens, 50) {
            for c in results {
                candidates.insert(c.market_id.clone(), c);
            }
        } else {
            warn!("BM25 search failed");
        }

        // 2. Semantic Search
        if let Ok(results) = self.market_index.search_semantic(raw_text, 50) {
            for c in results {
                // If exists, we could merge scores or keep the one with higher score.
                // For now, just insert if not present (union).
                // Or maybe we want to boost score if found in both?
                // Let's just add unique ones.
                candidates.entry(c.market_id.clone()).or_insert(c);
            }
        } else {
            warn!("Semantic search failed");
        }

        candidates.into_values().collect()
    }

    async fn ensure_market_data(&mut self, candidates: &[RawCandidate]) {
        let missing_ids: Vec<String> = candidates
            .iter()
            .map(|c| c.market_id.clone())
            .filter(|id| !self.market_data_cache.contains_key(id))
            .collect();

        if missing_ids.is_empty() {
            return;
        }

        // Subscribe to market data updates *before* sending requests to avoid races
        let mut rx = self.bus.market_data.subscribe();

        for id in &missing_ids {
            let req = MarketDataRequest {
                market_id: id.clone(),
            };
            if let Err(e) = self.bus.market_data_request.publish(req).await {
                error!("Failed to publish market data request: {}", e);
            }
        }

        // Wait for data with a timeout
        let timeout = tokio::time::sleep(std::time::Duration::from_millis(2000));
        tokio::pin!(timeout);

        loop {
            tokio::select! {
                _ = &mut timeout => {
                    warn!("Timeout waiting for market data");
                    break;
                }
                res = rx.recv() => {
                    match res {
                        Ok(snap) => {
                            self.market_data_cache.insert(snap.market_id.clone(), (*snap).clone());
                            if missing_ids.iter().all(|id| self.market_data_cache.contains_key(id)) {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        }
    }

    fn build_orders_from_sized_decisions(&self, sized: &[SizedDecision]) -> Vec<Order> {
        let mut orders = Vec::new();
        // Use configured bankroll
        let bankroll = self.bankroll;

        for decision in sized {
            if decision.size_fraction <= Decimal::ZERO {
                continue;
            }

            // Calculate quantity based on bankroll and price
            // size_fraction is the fraction of bankroll to risk/invest
            // quantity = (bankroll * size_fraction) / price
            let price = decision.candidate.market_price;
            if price <= Decimal::ZERO {
                continue;
            }

            let quantity = (bankroll * decision.size_fraction) / price;

            // Generate a simple client order ID
            let client_order_id = format!(
                "{}-{}",
                decision.candidate.candidate.market_id,
                Utc::now().timestamp_micros()
            );

            // Resolve token_id
            let mut token_id = None;
            if let Some(snap) = self
                .market_data_cache
                .get(&decision.candidate.candidate.market_id)
            {
                if let Some(tokens) = &snap.tokens {
                    let target_outcome = match decision.side {
                        TradeSide::BuyYes => "Yes",
                        TradeSide::BuyNo => "No",
                    };
                    // Case-insensitive match just in case
                    if let Some(t) = tokens
                        .iter()
                        .find(|t| t.outcome.eq_ignore_ascii_case(target_outcome))
                    {
                        token_id = Some(t.token_id.clone());
                    } else {
                        warn!(
                            "Token ID not found for outcome {} in market {}",
                            target_outcome, decision.candidate.candidate.market_id
                        );
                    }
                }
            }

            let order = Order {
                client_order_id,
                market_id: decision.candidate.candidate.market_id.clone(),
                token_id,
                side: crate::core::types::Side::Buy, // Always Buy side for the specific token
                price,
                size: quantity,
            };
            orders.push(order);
        }
        orders
    }

    #[allow(dead_code)]
    fn map_poly_event_to_news(&self, _evt: &PolyMarketEvent) -> RawNews {
        // TODO: convert PolyMarketEvent into a RawNews-like structure
        // Manually constructing RawNews since it doesn't implement Default
        RawNews {
            url: "".to_string(),
            title: "placeholder".to_string(),
            description: "".to_string(),
            feed: "polymarket".to_string(),
            published: Some(Utc::now()),
            labels: Vec::new(),
        }
    }

    async fn decide_from_news(&mut self, news: &RawNews) -> Option<Order> {
        // Full matching + decision pipeline for generic news.
        self.handle_news_event(news).await.into_iter().next()
    }

    fn decide_from_poly_event(&mut self, event: &PolyMarketEvent) -> Option<Order> {
        // Structural/reactive logic:
        // - reindex market if needed
        // - update cached metadata
        // - adjust risk/execution if event impacts position
        // - no "matching" pipeline here

        // If your PolyMarketEvent represents:
        // - Market creation → you should update your Tantivy BM25 index
        // - Market metadata update → re-index that specific market
        // - Market resolution → update calibration labels (score → outcome)
        // - Volume/liquidity update → risk & execution logic
        // - Odds movement → internal signal, not external news
        // Then sending it through the news matching pipeline makes no sense.
        //
        // Instead, it belongs in:
        // - index update logic
        // - calibration storage
        // - risk/execution adjustment logic

        // Update index if it's a market update/creation
        // Update index if it's a market update/creation
        if let Some(markets) = &event.markets {
            for market in markets {
                let question = market
                    .question
                    .as_deref()
                    .or(event.title.as_deref())
                    .unwrap_or("");
                let description = market
                    .description
                    .as_deref()
                    .or(event.description.as_deref())
                    .unwrap_or("");

                if !question.is_empty() {
                    if let Err(e) =
                        self.market_index
                            .add_market(&market.id, question, description, "", None)
                    {
                        error!("Failed to index market {}: {}", market.id, e);
                    }
                }
            }
        }
        None
    }

    fn decide_from_executions(&mut self, execution: &Execution) -> Option<Order> {
        info!("StrategyActor received execution: {:?}", execution);
        // High-level responsibilities:
        // - Update internal notion of open positions, risk, PnL.
        // - Possibly issue follow-up orders:
        //   * scale in/out
        //   * hedge correlated markets
        //   * reduce exposure if fills are larger than planned
        //
        // For now: delegate to a placeholder:
        None
    }
}

#[async_trait::async_trait]
impl Actor for StrategyActor {
    async fn run(mut self) -> Result<()> {
        info!("StrategyActor started");

        // Subscribe to both broadcast streams
        let mut md_rx = self.bus.market_data.subscribe(); // broadcast::Receiver<Arc<MarketDataSnap>>
        let mut poly_rx = self.bus.polymarket_events.subscribe(); // broadcast::Receiver<Arc<RawNews>>
        let mut news_rx = self.bus.raw_news.subscribe(); // broadcast::Receiver<Arc<RawNews>>
        let mut executions_rx = self.bus.executions.subscribe(); // broadcast::Receiver<Arc<Executions>>

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
                            if let Some(order) = self.decide_from_news(&news).await {
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

                // Polymarket events path
                res = poly_rx.recv() => {
                    match res {
                        Ok(event) => {
                            if let Some(order) = self.decide_from_poly_event(&event) {
                                self.bus.orders.publish(order).await?;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            warn!(lagged = n, "StrategyActor lagged on polymarket_events");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            error!("polymarket_events stream closed; exiting StrategyActor");
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

#[cfg(test)]
mod tests {
    use super::*;
    // use crate::config::config::CalibrationCfg;
    use crate::core::types::MarketToken;
    use crate::strategy::exact_duplicate_detector::ExactDuplicateDetectorConfig;
    use crate::strategy::sim_hash_cache::SimHashCacheConfig;
    use crate::strategy::types::{EdgedCandidate, RawCandidate, SizedDecision, TradeSide};

    #[test]
    fn test_token_id_resolution() {
        // Setup minimal actor
        let mut actor = StrategyActor {
            bus: Bus::new(),
            shutdown: CancellationToken::new(),
            detector: ExactDuplicateDetector::new(ExactDuplicateDetectorConfig::default()),
            sim_hash_cache: SimHashCache::new(SimHashCacheConfig::default()),
            event_feature_extractor: EventFeatureExtractor::new(
                FeatureDictionaries::default_minimal(),
            ),
            canonical_builder: CanonicalEventBuilder::new(),
            market_index: MarketIndex::new().unwrap(),
            hard_filterer: HardFilterer::new(),
            // scorer: Scorer::new(),
            // calibrator: ProbabilityCalibrator::new(CalibrationCfg::default()),
            kelly_sizer: KellySizer::default(),
            market_data_cache: HashMap::new(),
            bankroll: Decimal::from_f64(1000.0).unwrap(),
            llm_client: LlmClient::new(crate::config::config::LlmCfg::default()),
        };

        // Mock Market Data
        let market_id = "123456";
        let yes_token = "token_yes_123";
        let no_token = "token_no_123";

        let snap = MarketDataSnap {
            market_id: market_id.to_string(),
            book_ts_ms: 0,
            best_bid: Decimal::new(5, 1),   // 0.5
            best_ask: Decimal::new(6, 1),   // 0.6
            bid_size: Decimal::new(100, 0), // 100.0
            ask_size: Decimal::new(100, 0), // 100.0
            tokens: Some(vec![
                MarketToken {
                    token_id: yes_token.to_string(),
                    outcome: "Yes".to_string(),
                    price: Decimal::new(55, 2), // 0.55
                },
                MarketToken {
                    token_id: no_token.to_string(),
                    outcome: "No".to_string(),
                    price: Decimal::new(45, 2), // 0.45
                },
            ]),
            question: "Will the Fed hike rates?".to_string(),
        };
        actor.market_data_cache.insert(market_id.to_string(), snap);

        // Test BuyYes
        let decision_yes = SizedDecision {
            candidate: EdgedCandidate {
                candidate: RawCandidate {
                    market_id: market_id.to_string(),
                    ..Default::default()
                },
                score: Decimal::new(8, 1),         // 0.8
                probability: Decimal::new(7, 1),   // 0.7
                market_price: Decimal::new(55, 2), // 0.55
                edge: Decimal::new(15, 2),         // 0.15
            },
            kelly_fraction: Decimal::new(1, 1), // 0.1
            size_fraction: Decimal::new(1, 1),  // 0.1
            side: TradeSide::BuyYes,
        };

        let orders_yes = actor.build_orders_from_sized_decisions(&[decision_yes]);
        assert_eq!(orders_yes.len(), 1);
        assert_eq!(orders_yes[0].token_id, Some(yes_token.to_string()));

        // Test BuyNo
        let decision_no = SizedDecision {
            candidate: EdgedCandidate {
                candidate: RawCandidate {
                    market_id: market_id.to_string(),
                    ..Default::default()
                },
                score: Decimal::new(8, 1),         // 0.8
                probability: Decimal::new(3, 1),   // 0.3
                market_price: Decimal::new(45, 2), // 0.45
                edge: Decimal::new(15, 2),         // 0.15
            },
            kelly_fraction: Decimal::new(1, 1), // 0.1
            size_fraction: Decimal::new(1, 1),  // 0.1
            side: TradeSide::BuyNo,
        };

        let orders_no = actor.build_orders_from_sized_decisions(&[decision_no]);
        assert_eq!(orders_no.len(), 1);
        assert_eq!(orders_no[0].token_id, Some(no_token.to_string()));
    }
}
