use crate::services::error::{AppError, Result};
use crate::services::provider_v2::ProviderModel;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use uuid::Uuid;

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

const GENERIC_BASE_INSTRUCTIONS: &str = "You are a coding agent. Follow the provided developer and user instructions, use available tools carefully, and make verifiable changes in the current workspace.";
const BUILTIN_CODEX_MODEL_CATALOG: &str =
    include_str!("../../../resources/codex-model-catalog.json");

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_context_window: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_compact_token_limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_context_window_percent: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodexModelDefaultsCatalog {
    models: BTreeMap<String, CodexModelDefaults>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
struct CodexModelDefaults {
    slug: String,
    context_window: Option<i64>,
    max_context_window: Option<i64>,
    auto_compact_token_limit: Option<i64>,
    effective_context_window_percent: Option<i64>,
}

#[derive(Deserialize)]
struct CodexModelDefaultsDocument {
    #[serde(default)]
    models: Vec<CodexModelDefaults>,
}

impl CodexModelDefaultsCatalog {
    pub fn builtin() -> Result<Self> {
        Self::from_json(BUILTIN_CODEX_MODEL_CATALOG)
    }

    pub fn from_json(source: &str) -> Result<Self> {
        let document = serde_json::from_str::<CodexModelDefaultsDocument>(source)
            .map_err(|error| AppError::new("CODEX_MODEL_CATALOG_INVALID", error.to_string()))?;
        let models = document
            .models
            .into_iter()
            .filter(|model| !model.slug.trim().is_empty() && model.slug.trim() == model.slug)
            .map(|mut model| {
                model.context_window = positive(model.context_window);
                model.max_context_window = positive(model.max_context_window);
                model.auto_compact_token_limit = positive(model.auto_compact_token_limit);
                model.effective_context_window_percent = model
                    .effective_context_window_percent
                    .filter(|value| (1..=100).contains(value));
                (model.slug.clone(), model)
            })
            .collect();
        Ok(Self { models })
    }

    pub fn from_path(path: &std::path::Path, max_bytes: u64) -> Result<Self> {
        let metadata = std::fs::metadata(path)?;
        if metadata.len() > max_bytes {
            return Err(AppError::new(
                "CODEX_MODEL_CATALOG_TOO_LARGE",
                "Codex model catalog exceeds the configured size limit",
            ));
        }
        let source = std::fs::read_to_string(path)?;
        Self::from_json(&source)
    }

    pub fn overlay_json(self, source: &str) -> Result<Self> {
        Self::from_json(source).map(|overlay| self.overlay(overlay))
    }

    pub fn overlay(mut self, overlay: Self) -> Self {
        self.models.extend(overlay.models);
        self
    }

    fn get(&self, slug: &str) -> Option<&CodexModelDefaults> {
        self.models.get(slug)
    }
}

fn positive(value: Option<i64>) -> Option<i64> {
    value.filter(|value| *value > 0)
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

pub const CODEX_MODEL_CATALOG_FILE: &str = "models.json";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CodexTruncationPolicy {
    pub mode: &'static str,
    pub limit: usize,
}

pub fn build_codex_model_catalog(models: &[ProviderModel]) -> Result<CodexModelCatalog> {
    build_codex_model_catalog_with_defaults(models, &CodexModelDefaultsCatalog::default())
}

pub fn write_codex_model_catalog(codex_home: &Path, models: &[ProviderModel]) -> Result<()> {
    let catalog =
        build_codex_model_catalog_with_defaults(models, &CodexModelDefaultsCatalog::builtin()?)?;
    let mut body = serde_json::to_vec_pretty(&catalog).map_err(|error| {
        AppError::new(
            "CODEX_MODEL_CATALOG_SERIALIZATION_FAILED",
            error.to_string(),
        )
    })?;
    body.push(b'\n');
    fs::create_dir_all(codex_home)?;
    #[cfg(unix)]
    fs::set_permissions(codex_home, fs::Permissions::from_mode(0o700))?;
    let target = codex_home.join(CODEX_MODEL_CATALOG_FILE);
    let temp = codex_home.join(format!(".models.{}.tmp", Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&temp)?;
        file.write_all(&body)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, &target)?;
        #[cfg(unix)]
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

pub fn build_codex_model_catalog_with_defaults(
    models: &[ProviderModel],
    defaults: &CodexModelDefaultsCatalog,
) -> Result<CodexModelCatalog> {
    let mut models = models.to_vec();
    models.sort_by(|left, right| left.id.cmp(&right.id));
    validate_models(&models)?;
    Ok(CodexModelCatalog {
        models: models
            .into_iter()
            .enumerate()
            .map(|(index, model)| {
                let model_defaults = defaults.get(&model.id);
                codex_model(model, index + 1, model_defaults)
            })
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

fn codex_model(
    model: ProviderModel,
    priority: usize,
    defaults: Option<&CodexModelDefaults>,
) -> CodexModelInfo {
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
        context_window: defaults.and_then(|value| value.context_window),
        max_context_window: defaults.and_then(|value| value.max_context_window),
        auto_compact_token_limit: defaults.and_then(|value| value.auto_compact_token_limit),
        effective_context_window_percent: defaults
            .and_then(|value| value.effective_context_window_percent),
    }
}
