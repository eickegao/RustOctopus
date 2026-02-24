use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use super::registry::{find_by_model, find_gateway, ProviderSpec};
use super::traits::*;

/// A unified OpenAI-compatible HTTP client that handles all LLM providers.
///
/// Differences between providers (endpoint URLs, model prefixes, auth quirks)
/// are driven by `ProviderSpec` from the registry — no if-elif chains.
pub struct OpenAiCompatClient {
    http: reqwest::Client,
    api_key: String,
    api_base: Option<String>,
    default_model: String,
    extra_headers: HashMap<String, String>,
    gateway: Option<&'static ProviderSpec>,
}

// ── Request types ────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: String,
    messages: &'a [ChatMessage],
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<&'a [ToolDefinition]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
    max_tokens: u32,
    temperature: f64,
}

// ── Response types ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<ResponseToolCall>>,
    reasoning_content: Option<String>,
}

#[derive(Deserialize)]
struct ResponseToolCall {
    id: String,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    call_type: String,
    function: ResponseToolCallFunction,
}

#[derive(Deserialize)]
struct ResponseToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct Usage {
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    total_tokens: Option<u32>,
}

// ── Implementation ───────────────────────────────────────────────────────────

impl OpenAiCompatClient {
    /// Create a new client.
    ///
    /// `provider_name` is an optional hint used to detect a gateway/local spec.
    pub fn new(
        api_key: String,
        api_base: Option<String>,
        default_model: String,
        extra_headers: HashMap<String, String>,
        provider_name: Option<&str>,
    ) -> Self {
        let gateway = find_gateway(
            provider_name,
            Some(&api_key),
            api_base.as_deref(),
        );

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .expect("failed to build HTTP client");

        Self {
            http,
            api_key,
            api_base,
            default_model,
            extra_headers,
            gateway,
        }
    }

    /// Test constructor that does not need real HTTP configuration.
    #[cfg(test)]
    pub fn new_for_test(provider_name: Option<&str>, api_key: &str) -> Self {
        let gateway = find_gateway(
            provider_name,
            Some(api_key),
            None,
        );

        Self {
            http: reqwest::Client::new(),
            api_key: api_key.to_string(),
            api_base: None,
            default_model: String::new(),
            extra_headers: HashMap::new(),
            gateway,
        }
    }

    /// Resolve the model name by applying provider-specific prefix rules.
    ///
    /// - **Gateway mode**: optionally strip `provider/` prefix from the model,
    ///   then add the gateway's `model_prefix` if non-empty and not already present.
    /// - **Standard mode**: look up the provider by model name, then add its
    ///   `model_prefix` if non-empty, not already present, and the model
    ///   doesn't start with any known skip prefix (i.e., another provider's prefix).
    pub fn resolve_model(&self, model: &str) -> String {
        if let Some(gw) = self.gateway {
            // Gateway mode
            let mut m = model.to_string();

            // Strip "provider/" prefix if the gateway spec says to
            if gw.strip_model_prefix {
                if let Some(pos) = m.find('/') {
                    m = m[pos + 1..].to_string();
                }
            }

            // Add gateway model_prefix if non-empty and not already prefixed
            if !gw.model_prefix.is_empty() {
                let prefix_with_slash = format!("{}/", gw.model_prefix);
                if !m.starts_with(&prefix_with_slash) {
                    m = format!("{}/{}", gw.model_prefix, m);
                }
            }

            m
        } else {
            // Standard mode: look up the provider by model name
            if let Some(spec) = find_by_model(model) {
                if !spec.model_prefix.is_empty() {
                    let prefix_with_slash = format!("{}/", spec.model_prefix);
                    // Don't add prefix if model already has it
                    if model.starts_with(&prefix_with_slash) {
                        return model.to_string();
                    }
                    // Don't add prefix if the model already has a different
                    // known provider prefix (e.g., "anthropic/claude-..." should
                    // not get an additional prefix).
                    if model.contains('/') {
                        return model.to_string();
                    }
                    return format!("{}/{}", spec.model_prefix, model);
                }
            }
            model.to_string()
        }
    }

    /// Resolve the chat completions endpoint URL.
    ///
    /// Priority:
    /// 1. Explicit `api_base` from constructor
    /// 2. Gateway spec's `default_api_base`
    /// 3. Standard provider spec's `default_api_base` (looked up by default model)
    /// 4. Fallback: `https://api.openai.com/v1`
    pub fn resolve_endpoint(&self) -> String {
        let base = if let Some(ref ab) = self.api_base {
            ab.clone()
        } else if let Some(gw) = self.gateway {
            if !gw.default_api_base.is_empty() {
                gw.default_api_base.to_string()
            } else {
                self.fallback_base()
            }
        } else {
            self.fallback_base()
        };

        let base = base.trim_end_matches('/');
        format!("{}/chat/completions", base)
    }

    /// Fallback: try to find a base URL from the default_model's spec, else OpenAI.
    fn fallback_base(&self) -> String {
        if !self.default_model.is_empty() {
            if let Some(spec) = find_by_model(&self.default_model) {
                if !spec.default_api_base.is_empty() {
                    return spec.default_api_base.to_string();
                }
            }
        }
        "https://api.openai.com/v1".to_string()
    }
}

#[async_trait]
impl LlmProvider for OpenAiCompatClient {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[ToolDefinition]>,
        model: &str,
        params: &ChatParams,
    ) -> anyhow::Result<LlmResponse> {
        let resolved_model = self.resolve_model(model);
        let endpoint = self.resolve_endpoint();

        debug!(
            model = %resolved_model,
            endpoint = %endpoint,
            message_count = messages.len(),
            "sending chat request"
        );

        let tool_choice = tools
            .filter(|t| !t.is_empty())
            .map(|_| "auto".to_string());

        let body = ChatRequest {
            model: resolved_model,
            messages,
            tools: tools.filter(|t| !t.is_empty()),
            tool_choice,
            max_tokens: params.max_tokens,
            temperature: params.temperature,
        };

        let mut req = self
            .http
            .post(&endpoint)
            .bearer_auth(&self.api_key)
            .json(&body);

        for (k, v) in &self.extra_headers {
            req = req.header(k, v);
        }

        let resp = req.send().await?;

        let status = resp.status();
        if !status.is_success() {
            let error_body = resp.text().await.unwrap_or_default();
            warn!(
                status = %status,
                body = %error_body,
                "LLM API returned error"
            );
            anyhow::bail!("LLM API error {}: {}", status, error_body);
        }

        let completion: ChatCompletionResponse = resp.json().await?;

        // Parse the first choice
        let choice = completion
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("LLM returned no choices"))?;

        let content = choice.message.content;
        let reasoning_content = choice.message.reasoning_content;

        // Parse tool calls, converting JSON string arguments to serde_json::Value
        let tool_calls = match choice.message.tool_calls {
            Some(calls) => {
                let mut parsed = Vec::with_capacity(calls.len());
                for tc in calls {
                    let arguments: serde_json::Value =
                        serde_json::from_str(&tc.function.arguments).unwrap_or_else(|e| {
                            warn!(
                                name = %tc.function.name,
                                error = %e,
                                "failed to parse tool call arguments as JSON"
                            );
                            serde_json::Value::Object(serde_json::Map::new())
                        });
                    parsed.push(ToolCallRequest {
                        id: tc.id,
                        name: tc.function.name,
                        arguments,
                    });
                }
                parsed
            }
            None => Vec::new(),
        };

        let finish_reason = match choice.finish_reason.as_deref() {
            Some("tool_calls") => FinishReason::ToolCalls,
            Some("length") | Some("max_tokens") => FinishReason::MaxTokens,
            Some("stop") | None => {
                if !tool_calls.is_empty() {
                    FinishReason::ToolCalls
                } else {
                    FinishReason::Stop
                }
            }
            Some(other) => {
                debug!(reason = %other, "unknown finish_reason, treating as Stop");
                FinishReason::Stop
            }
        };

        let usage = match completion.usage {
            Some(u) => TokenUsage {
                prompt_tokens: u.prompt_tokens.unwrap_or(0),
                completion_tokens: u.completion_tokens.unwrap_or(0),
                total_tokens: u.total_tokens.unwrap_or(0),
            },
            None => TokenUsage::default(),
        };

        Ok(LlmResponse {
            content,
            tool_calls,
            finish_reason,
            usage,
            reasoning_content,
        })
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── resolve_model tests ──────────────────────────────────────────────

    #[test]
    fn test_resolve_model_gateway_openrouter() {
        // OpenRouter is a gateway with model_prefix="openrouter", strip_model_prefix=false.
        // When a model like "anthropic/claude-sonnet-4-5" is used through OpenRouter,
        // the gateway adds "openrouter/" prefix since model_prefix is non-empty.
        // But if the model already has the "openrouter/" prefix, it stays as-is.
        let client = OpenAiCompatClient::new_for_test(
            Some("openrouter"),
            "sk-or-test123",
        );
        assert!(client.gateway.is_some());
        assert_eq!(client.gateway.unwrap().name, "openrouter");

        // Model already prefixed with "openrouter/" stays as-is
        assert_eq!(
            client.resolve_model("openrouter/anthropic/claude-sonnet-4-5"),
            "openrouter/anthropic/claude-sonnet-4-5"
        );

        // Model without openrouter prefix gets it added
        assert_eq!(
            client.resolve_model("anthropic/claude-sonnet-4-5"),
            "openrouter/anthropic/claude-sonnet-4-5"
        );
    }

    #[test]
    fn test_resolve_model_gateway_aihubmix() {
        // AiHubMix: model_prefix="openai", strip_model_prefix=true.
        // First strip the "provider/" prefix, then add "openai/" if not present.
        let client = OpenAiCompatClient {
            http: reqwest::Client::new(),
            api_key: "test".to_string(),
            api_base: Some("https://aihubmix.com/v1".to_string()),
            default_model: String::new(),
            extra_headers: HashMap::new(),
            gateway: crate::providers::registry::find_gateway(
                None,
                None,
                Some("https://aihubmix.com/v1"),
            ),
        };
        assert!(client.gateway.is_some());
        assert_eq!(client.gateway.unwrap().name, "aihubmix");

        // "anthropic/claude-sonnet-4-5" -> strip prefix -> "claude-sonnet-4-5"
        // -> add "openai/" -> "openai/claude-sonnet-4-5"
        assert_eq!(
            client.resolve_model("anthropic/claude-sonnet-4-5"),
            "openai/claude-sonnet-4-5"
        );

        // Already has the "openai/" prefix after stripping: don't double-prefix
        assert_eq!(
            client.resolve_model("openai/gpt-4"),
            "openai/gpt-4"
        );
    }

    #[test]
    fn test_resolve_model_deepseek() {
        // Standard mode: DeepSeek has model_prefix="deepseek".
        // "deepseek-chat" -> "deepseek/deepseek-chat"
        let client = OpenAiCompatClient::new_for_test(None, "test-key");
        assert!(client.gateway.is_none());

        assert_eq!(
            client.resolve_model("deepseek-chat"),
            "deepseek/deepseek-chat"
        );
    }

    #[test]
    fn test_resolve_model_no_double_prefix() {
        // "deepseek/deepseek-chat" already has the prefix -> stays as-is
        let client = OpenAiCompatClient::new_for_test(None, "test-key");
        assert_eq!(
            client.resolve_model("deepseek/deepseek-chat"),
            "deepseek/deepseek-chat"
        );
    }

    #[test]
    fn test_resolve_model_anthropic_no_prefix() {
        // Anthropic has model_prefix="" -> no prefix added.
        // "anthropic/claude-sonnet-4-5" stays as-is.
        let client = OpenAiCompatClient::new_for_test(None, "test-key");
        assert_eq!(
            client.resolve_model("anthropic/claude-sonnet-4-5"),
            "anthropic/claude-sonnet-4-5"
        );
    }

    // ── resolve_endpoint tests ───────────────────────────────────────────

    #[test]
    fn test_resolve_endpoint_with_api_base() {
        // Custom api_base takes precedence over everything
        let client = OpenAiCompatClient {
            http: reqwest::Client::new(),
            api_key: "test".to_string(),
            api_base: Some("https://my-custom-server.com/v1".to_string()),
            default_model: String::new(),
            extra_headers: HashMap::new(),
            gateway: None,
        };
        assert_eq!(
            client.resolve_endpoint(),
            "https://my-custom-server.com/v1/chat/completions"
        );
    }

    #[test]
    fn test_resolve_endpoint_gateway_default() {
        // Gateway spec's default_api_base is used when no api_base is set
        let client = OpenAiCompatClient::new_for_test(
            Some("openrouter"),
            "sk-or-test123",
        );
        assert_eq!(
            client.resolve_endpoint(),
            "https://openrouter.ai/api/v1/chat/completions"
        );
    }

    #[test]
    fn test_resolve_endpoint_openai_fallback() {
        // No spec, no api_base -> fallback to OpenAI
        let client = OpenAiCompatClient {
            http: reqwest::Client::new(),
            api_key: "test".to_string(),
            api_base: None,
            default_model: String::new(),
            extra_headers: HashMap::new(),
            gateway: None,
        };
        assert_eq!(
            client.resolve_endpoint(),
            "https://api.openai.com/v1/chat/completions"
        );
    }
}
