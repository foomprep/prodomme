use reqwest::Client;
use serde::{Serialize, Deserialize};
use anyhow::Result;

use crate::{config::ProjectConfig, tooler::Tooler};
use super::types::{
    ContentItem, InferenceError, Message, ModelResponse, Usage
};

#[derive(Serialize)]
struct AnthropicRequest<'a> {
    model: &'a str,
    messages: Vec<Message>,
    max_tokens: u32,
    tools: serde_json::Value,
    system: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    id: String,
    model: String,
    role: String,
    content: Vec<AnthropicContentItem>,
    stop_reason: String,
    stop_sequence: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Debug, Deserialize)]
struct AnthropicContentItem {
    text: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: i32,
    output_tokens: i32,
}

pub struct AnthropicInference {
    model: String,
    client: Client,
    tooler: Tooler,
    base_url: String,
    api_key: String,
    max_output_tokens: u32,
}

impl std::default::Default for AnthropicInference {
    fn default() -> Self {
        let config = match ProjectConfig::load() {
            Ok(config) => config,
            Err(_) => ProjectConfig::default(),
        };
        
        AnthropicInference {
            model: config.model,
            client: Client::new(),
            tooler: Tooler::new(),
            base_url: "https://api.anthropic.com/v1".to_string(),
            api_key: config.api_key,
            max_output_tokens: config.max_output_tokens,
        }
    }
}

impl AnthropicInference {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn query_model(&self, messages: Vec<Message>, system_message: Option<&str>) -> Result<ModelResponse, InferenceError> {
        if self.api_key.is_empty() {
            return Err(InferenceError::MissingApiKey("Anthropic API key not found".to_string()));
        }

        let system = system_message.unwrap_or("").to_string();

        let tools = self.tooler.get_tools_json()
            .map_err(|e| InferenceError::SerializationError(e.to_string()))?;

        let request = AnthropicRequest {
            model: &self.model,
            messages,
            max_tokens: self.max_output_tokens,
            tools,
            system,
        };

        let response = self.client
            .post(format!("{}/messages", self.base_url))
            .header("Content-Type", "application/json")
            .header("X-API-Key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&request)
            .send()
            .await
            .map_err(|e| InferenceError::NetworkError(e.to_string()))?;

        let status = response.status();
        let response_text = response.text().await
            .map_err(|e| InferenceError::NetworkError(e.to_string()))?;

        if !status.is_success() {
            return Err(InferenceError::ApiError(status, response_text));
        }

        let anthropic_response: AnthropicResponse = serde_json::from_str(&response_text)
            .map_err(|e| InferenceError::InvalidResponse(e.to_string()))?;

        Ok(ModelResponse {
            content: vec![ContentItem::Text {
                text: anthropic_response.content.first()
                    .map(|item| item.text.clone())
                    .unwrap_or_default()
            }],
            id: anthropic_response.id,
            model: anthropic_response.model,
            role: anthropic_response.role,
            message_type: "text".to_string(),
            stop_reason: anthropic_response.stop_reason,
            stop_sequence: anthropic_response.stop_sequence,
            usage: Usage {
                input_tokens: anthropic_response.usage.input_tokens,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
                output_tokens: anthropic_response.usage.output_tokens,
            },
        })
    }
}