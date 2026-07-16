use crate::services::error::{AppError, Result};
use crate::services::provider_v2::ProviderModel;
use serde::Serialize;
use std::collections::BTreeSet;

const GENERIC_BASE_INSTRUCTIONS: &str = "You are a coding agent. Follow the provided developer and user instructions, use available tools carefully, and make verifiable changes in the current workspace.";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CodexModelCatalog {
    pub models: Vec<CodexModelInfo>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CodexModelInfo {
    pub slug: String,
    pub display_name: String,
    pub supported_reasoning_levels: Vec<CodexReasoningLevel>,
    pub shell_type: CodexShellType,
    pub visibility: CodexModelVisibility,
    pub supported_in_api: bool,
    pub priority: usize,
    pub base_instructions: &'static str,
    pub supports_reasoning_summaries: bool,
    pub support_verbosity: bool,
    pub truncation_policy: CodexTruncationPolicy,
    pub supports_parallel_tool_calls: bool,
    pub experimental_supported_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CodexReasoningLevel {
    pub effort: String,
    pub description: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodexShellType {
    ShellCommand,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodexModelVisibility {
    List,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CodexTruncationPolicy {
    pub mode: &'static str,
    pub limit: usize,
}

pub fn build_codex_model_catalog(models: &[ProviderModel]) -> Result<CodexModelCatalog> {
    let mut models = models.to_vec();
    models.sort_by(|left, right| left.id.cmp(&right.id));
    validate_models(&models)?;
    Ok(CodexModelCatalog {
        models: models
            .into_iter()
            .enumerate()
            .map(|(index, model)| codex_model(model, index + 1))
            .collect(),
    })
}

fn validate_models(models: &[ProviderModel]) -> Result<()> {
    let mut slugs = BTreeSet::new();
    for model in models {
        if model.id.trim().is_empty() || model.id.trim() != model.id {
            return Err(AppError::new(
                "CODEX_MODEL_SLUG_INVALID",
                "invalid model slug",
            ));
        }
        if !slugs.insert(model.id.as_str()) {
            return Err(AppError::new(
                "CODEX_MODEL_SLUG_DUPLICATE",
                "duplicate model slug",
            ));
        }
    }
    Ok(())
}

fn codex_model(model: ProviderModel, priority: usize) -> CodexModelInfo {
    CodexModelInfo {
        slug: model.id,
        display_name: model.label,
        supported_reasoning_levels: Vec::new(),
        shell_type: CodexShellType::ShellCommand,
        visibility: CodexModelVisibility::List,
        supported_in_api: true,
        priority,
        base_instructions: GENERIC_BASE_INSTRUCTIONS,
        supports_reasoning_summaries: false,
        support_verbosity: false,
        truncation_policy: CodexTruncationPolicy {
            mode: "tokens",
            limit: 10_000,
        },
        supports_parallel_tool_calls: false,
        experimental_supported_tools: Vec::new(),
    }
}
