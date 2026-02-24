/// Static registry of LLM provider specifications.
///
/// Order matters: it controls match priority and fallback.
/// Gateways first, then standard providers, then local, then auxiliary.

#[derive(Debug, Clone)]
pub struct ProviderSpec {
    pub name: &'static str,
    pub keywords: &'static [&'static str],
    pub env_key: &'static str,
    pub display_name: &'static str,
    pub default_api_base: &'static str,
    pub model_prefix: &'static str,
    pub strip_model_prefix: bool,
    pub is_gateway: bool,
    pub is_local: bool,
    pub is_oauth: bool,
    pub supports_prompt_caching: bool,
    pub detect_by_key_prefix: &'static str,
    pub detect_by_base_keyword: &'static str,
}

/// The provider registry. Order = priority.
pub static PROVIDERS: &[ProviderSpec] = &[
    // === Gateways (detected by api_key / api_base, not model name) =========

    // OpenRouter: global gateway, keys start with "sk-or-"
    ProviderSpec {
        name: "openrouter",
        keywords: &["openrouter"],
        env_key: "OPENROUTER_API_KEY",
        display_name: "OpenRouter",
        default_api_base: "https://openrouter.ai/api/v1",
        model_prefix: "openrouter",
        strip_model_prefix: false,
        is_gateway: true,
        is_local: false,
        is_oauth: false,
        supports_prompt_caching: true,
        detect_by_key_prefix: "sk-or-",
        detect_by_base_keyword: "openrouter",
    },
    // AiHubMix: global gateway, OpenAI-compatible interface.
    ProviderSpec {
        name: "aihubmix",
        keywords: &["aihubmix"],
        env_key: "OPENAI_API_KEY",
        display_name: "AiHubMix",
        default_api_base: "https://aihubmix.com/v1",
        model_prefix: "openai",
        strip_model_prefix: true,
        is_gateway: true,
        is_local: false,
        is_oauth: false,
        supports_prompt_caching: false,
        detect_by_key_prefix: "",
        detect_by_base_keyword: "aihubmix",
    },
    // SiliconFlow: OpenAI-compatible gateway
    ProviderSpec {
        name: "siliconflow",
        keywords: &["siliconflow"],
        env_key: "OPENAI_API_KEY",
        display_name: "SiliconFlow",
        default_api_base: "https://api.siliconflow.cn/v1",
        model_prefix: "openai",
        strip_model_prefix: false,
        is_gateway: true,
        is_local: false,
        is_oauth: false,
        supports_prompt_caching: false,
        detect_by_key_prefix: "",
        detect_by_base_keyword: "siliconflow",
    },
    // VolcEngine: OpenAI-compatible gateway
    ProviderSpec {
        name: "volcengine",
        keywords: &["volcengine", "volces", "ark"],
        env_key: "OPENAI_API_KEY",
        display_name: "VolcEngine",
        default_api_base: "https://ark.cn-beijing.volces.com/api/v3",
        model_prefix: "volcengine",
        strip_model_prefix: false,
        is_gateway: true,
        is_local: false,
        is_oauth: false,
        supports_prompt_caching: false,
        detect_by_key_prefix: "",
        detect_by_base_keyword: "volces",
    },

    // === Standard providers (matched by model-name keywords) ===============

    // Anthropic
    ProviderSpec {
        name: "anthropic",
        keywords: &["anthropic", "claude"],
        env_key: "ANTHROPIC_API_KEY",
        display_name: "Anthropic",
        default_api_base: "",
        model_prefix: "",
        strip_model_prefix: false,
        is_gateway: false,
        is_local: false,
        is_oauth: false,
        supports_prompt_caching: true,
        detect_by_key_prefix: "",
        detect_by_base_keyword: "",
    },
    // OpenAI
    ProviderSpec {
        name: "openai",
        keywords: &["openai", "gpt"],
        env_key: "OPENAI_API_KEY",
        display_name: "OpenAI",
        default_api_base: "",
        model_prefix: "",
        strip_model_prefix: false,
        is_gateway: false,
        is_local: false,
        is_oauth: false,
        supports_prompt_caching: false,
        detect_by_key_prefix: "",
        detect_by_base_keyword: "",
    },
    // DeepSeek
    ProviderSpec {
        name: "deepseek",
        keywords: &["deepseek"],
        env_key: "DEEPSEEK_API_KEY",
        display_name: "DeepSeek",
        default_api_base: "",
        model_prefix: "deepseek",
        strip_model_prefix: false,
        is_gateway: false,
        is_local: false,
        is_oauth: false,
        supports_prompt_caching: false,
        detect_by_key_prefix: "",
        detect_by_base_keyword: "",
    },
    // Gemini
    ProviderSpec {
        name: "gemini",
        keywords: &["gemini"],
        env_key: "GEMINI_API_KEY",
        display_name: "Gemini",
        default_api_base: "",
        model_prefix: "gemini",
        strip_model_prefix: false,
        is_gateway: false,
        is_local: false,
        is_oauth: false,
        supports_prompt_caching: false,
        detect_by_key_prefix: "",
        detect_by_base_keyword: "",
    },
    // Zhipu AI
    ProviderSpec {
        name: "zhipu",
        keywords: &["zhipu", "glm", "zai"],
        env_key: "ZAI_API_KEY",
        display_name: "Zhipu AI",
        default_api_base: "",
        model_prefix: "zai",
        strip_model_prefix: false,
        is_gateway: false,
        is_local: false,
        is_oauth: false,
        supports_prompt_caching: false,
        detect_by_key_prefix: "",
        detect_by_base_keyword: "",
    },
    // DashScope (Qwen models)
    ProviderSpec {
        name: "dashscope",
        keywords: &["qwen", "dashscope"],
        env_key: "DASHSCOPE_API_KEY",
        display_name: "DashScope",
        default_api_base: "",
        model_prefix: "dashscope",
        strip_model_prefix: false,
        is_gateway: false,
        is_local: false,
        is_oauth: false,
        supports_prompt_caching: false,
        detect_by_key_prefix: "",
        detect_by_base_keyword: "",
    },
    // Moonshot (Kimi models)
    ProviderSpec {
        name: "moonshot",
        keywords: &["moonshot", "kimi"],
        env_key: "MOONSHOT_API_KEY",
        display_name: "Moonshot",
        default_api_base: "https://api.moonshot.ai/v1",
        model_prefix: "moonshot",
        strip_model_prefix: false,
        is_gateway: false,
        is_local: false,
        is_oauth: false,
        supports_prompt_caching: false,
        detect_by_key_prefix: "",
        detect_by_base_keyword: "",
    },
    // MiniMax
    ProviderSpec {
        name: "minimax",
        keywords: &["minimax"],
        env_key: "MINIMAX_API_KEY",
        display_name: "MiniMax",
        default_api_base: "https://api.minimax.io/v1",
        model_prefix: "minimax",
        strip_model_prefix: false,
        is_gateway: false,
        is_local: false,
        is_oauth: false,
        supports_prompt_caching: false,
        detect_by_key_prefix: "",
        detect_by_base_keyword: "",
    },

    // === Local deployment ===================================================

    // vLLM / any OpenAI-compatible local server
    ProviderSpec {
        name: "vllm",
        keywords: &["vllm"],
        env_key: "HOSTED_VLLM_API_KEY",
        display_name: "vLLM/Local",
        default_api_base: "",
        model_prefix: "hosted_vllm",
        strip_model_prefix: false,
        is_gateway: false,
        is_local: true,
        is_oauth: false,
        supports_prompt_caching: false,
        detect_by_key_prefix: "",
        detect_by_base_keyword: "",
    },

    // === Auxiliary ===========================================================

    // Groq: mainly for Whisper voice transcription, also usable for LLM
    ProviderSpec {
        name: "groq",
        keywords: &["groq"],
        env_key: "GROQ_API_KEY",
        display_name: "Groq",
        default_api_base: "",
        model_prefix: "groq",
        strip_model_prefix: false,
        is_gateway: false,
        is_local: false,
        is_oauth: false,
        supports_prompt_caching: false,
        detect_by_key_prefix: "",
        detect_by_base_keyword: "",
    },
];

/// Match a standard provider by model-name keyword (case-insensitive).
/// Skips gateways and local providers -- those are matched by api_key/api_base instead.
///
/// Priority:
/// 1. Explicit prefix match: "deepseek/deepseek-chat" matches deepseek by prefix "deepseek".
/// 2. Keyword match: "deepseek-chat" matches deepseek by keyword "deepseek".
pub fn find_by_model(model: &str) -> Option<&'static ProviderSpec> {
    let model_lower = model.to_lowercase();
    let model_normalized = model_lower.replace('-', "_");

    let model_prefix = if model_lower.contains('/') {
        model_lower.split('/').next().unwrap_or("")
    } else {
        ""
    };
    let normalized_prefix = model_prefix.replace('-', "_");

    let std_specs: Vec<&ProviderSpec> = PROVIDERS
        .iter()
        .filter(|s| !s.is_gateway && !s.is_local)
        .collect();

    // 1. Prefer explicit provider prefix
    if !model_prefix.is_empty() {
        for spec in &std_specs {
            if normalized_prefix == spec.name {
                return Some(spec);
            }
        }
    }

    // 2. Keyword match
    for spec in &std_specs {
        for kw in spec.keywords {
            let kw_normalized = kw.replace('-', "_");
            if model_lower.contains(kw) || model_normalized.contains(&kw_normalized) {
                return Some(spec);
            }
        }
    }

    None
}

/// Detect gateway or local provider.
///
/// Priority:
/// 1. name -- if it maps to a gateway/local spec, use it directly.
/// 2. api_key prefix -- e.g. "sk-or-" matches OpenRouter.
/// 3. api_base keyword -- e.g. "aihubmix" in URL matches AiHubMix.
pub fn find_gateway(
    name: Option<&str>,
    api_key: Option<&str>,
    api_base: Option<&str>,
) -> Option<&'static ProviderSpec> {
    // 1. Direct match by config key
    if let Some(n) = name {
        if let Some(spec) = find_by_name(n) {
            if spec.is_gateway || spec.is_local {
                return Some(spec);
            }
        }
    }

    // 2. Auto-detect by api_key prefix / api_base keyword
    for spec in PROVIDERS {
        if !spec.detect_by_key_prefix.is_empty() {
            if let Some(key) = api_key {
                if key.starts_with(spec.detect_by_key_prefix) {
                    return Some(spec);
                }
            }
        }
        if !spec.detect_by_base_keyword.is_empty() {
            if let Some(base) = api_base {
                if base.contains(spec.detect_by_base_keyword) {
                    return Some(spec);
                }
            }
        }
    }

    None
}

/// Find a provider spec by config field name, e.g. "dashscope".
pub fn find_by_name(name: &str) -> Option<&'static ProviderSpec> {
    PROVIDERS.iter().find(|s| s.name == name)
}
