use crate::config::config::LlmCfg;
use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::info;

#[derive(Clone)]
pub struct LlmClient {
    client: Client,
    cfg: LlmCfg,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SignalResponse {
    pub sentiment: String, // "Positive", "Negative", "Neutral"
    pub confidence: f64,   // 0.0 to 1.0
    pub reasoning: String,
}

impl LlmClient {
    pub fn new(cfg: LlmCfg) -> Self {
        Self {
            client: Client::new(),
            cfg,
        }
    }

    pub async fn analyze(&self, news_title: &str, market_question: &str) -> Result<SignalResponse> {
        let prompt = format!(
            "You are a financial analyst specializing in event-driven market prediction. Analyze the following news in the context of the prediction market question.

            News: \"{}\"
            Market Question: \"{}\"

            Perform the following analysis step-by-step:
            1. Identify the key entities and events in the news.
            2. Compare this against historical precedents or market expectations.
            3. List 3 reasons why this news might imply 'Yes' (increases probability).
            4. List 3 reasons why this news might imply 'No' (decreases probability).
            5. Assign a weight (1-10) to each reason based on its impact.
            6. Synthesize these reasons into a final probability adjustment.

            Based on this analysis, determine the sentiment and new confidence level.
            
            Output strictly valid JSON with fields: 
            - 'sentiment' (Positive/Negative/Neutral), 
            - 'confidence' (0.0 to 1.0, representing the strength of the move), 
            - 'reasoning' (A concise summary of your step-by-step analysis).

            'Positive' means 'Yes' is more likely. 'Negative' means 'No' is more likely (or 'Yes' is less likely).",
            news_title, market_question
        );

        let req_body = json!({
            "model": self.cfg.model,
            "messages": [
                {"role": "system", "content": "You are a helpful assistant that outputs JSON."},
                {"role": "user", "content": prompt}
            ],
            "temperature": 0.0
        });

        let url = format!("{}/chat/completions", self.cfg.base_url);

        // Log for debugging (don't log full key in prod)
        info!("Calling LLM at {} with model {}", url, self.cfg.model);

        let res = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.cfg.api_key))
            .json(&req_body)
            .send()
            .await
            .context("LLM request failed")?;

        if !res.status().is_success() {
            let err_text = res.text().await?;
            anyhow::bail!("LLM API error: {}", err_text);
        }

        let resp_json: serde_json::Value = res.json().await?;

        // Extract content from OpenAI-like response
        let content_str = resp_json["choices"][0]["message"]["content"]
            .as_str()
            .context("No content in LLM response")?;

        // Parse JSON from content (handle potential markdown code blocks)
        let clean_content = content_str
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```");

        let signal: SignalResponse = serde_json::from_str(clean_content)
            .context(format!("Failed to parse LLM JSON: {}", clean_content))?;

        Ok(signal)
    }
}
