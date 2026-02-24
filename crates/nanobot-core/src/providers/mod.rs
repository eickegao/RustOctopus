pub mod openai_compat;
pub mod registry;
pub mod traits;

pub use traits::*;

#[cfg(test)]
mod tests {
    use super::registry::*;

    #[test]
    fn test_find_by_model_anthropic() {
        let spec = find_by_model("anthropic/claude-sonnet-4-5").unwrap();
        assert_eq!(spec.name, "anthropic");
    }

    #[test]
    fn test_find_by_model_deepseek() {
        let spec = find_by_model("deepseek-chat").unwrap();
        assert_eq!(spec.name, "deepseek");
    }

    #[test]
    fn test_find_by_model_qwen() {
        let spec = find_by_model("qwen-max").unwrap();
        assert_eq!(spec.name, "dashscope");
    }

    #[test]
    fn test_find_by_model_explicit_prefix() {
        let spec = find_by_model("gemini/gemini-pro").unwrap();
        assert_eq!(spec.name, "gemini");
    }

    #[test]
    fn test_find_gateway_by_key_prefix() {
        let spec = find_gateway(None, Some("sk-or-abc123"), None).unwrap();
        assert_eq!(spec.name, "openrouter");
    }

    #[test]
    fn test_find_gateway_by_base_keyword() {
        let spec = find_gateway(None, None, Some("https://aihubmix.com/v1")).unwrap();
        assert_eq!(spec.name, "aihubmix");
    }

    #[test]
    fn test_find_gateway_by_name() {
        let spec = find_gateway(Some("vllm"), None, None).unwrap();
        assert!(spec.is_local);
    }

    #[test]
    fn test_find_by_name() {
        let spec = find_by_name("dashscope").unwrap();
        assert_eq!(spec.display_name, "DashScope");
    }

    #[test]
    fn test_find_by_model_unknown() {
        assert!(find_by_model("unknown-model-xyz").is_none());
    }
}
