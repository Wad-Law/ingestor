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
            "You are a financial analyst. Analyze the following news title in the context of the prediction market question.\n\
            News: \"{}\"\n\
            Market Question: \"{}\"\n\
            \n\
            Determine if the news increases the probability of the outcome 'Yes', decreases it, or is neutral.\n\
            Output JSON with fields: 'sentiment' (Positive/Negative/Neutral), 'confidence' (0.0-1.0), and 'reasoning'.\n\
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
