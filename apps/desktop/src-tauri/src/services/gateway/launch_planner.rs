use crate::services::error::{AppError, Result};
use crate::services::provider_binding::RouteKind;
use crate::services::provider_binding::{ProfileBindingCollection, ProfileBindingRepository};
use crate::services::storage::{InstallationLock, StoreOptions, VersionedFileStore};
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchEntry {
    Normal { args: Vec<String> },
    Resume { session_id: Option<String> },
    ExecResume { session_id: String, prompt: String },
    RelayHandoff { session_id: String, prompt: String },
    Terminal { args: Vec<String> },
    CopyCommand { args: Vec<String> },
}

pub fn resolve_profile_route(home_root: &Path, profile_id: &str) -> Result<RouteKind> {
    validate_profile_id(profile_id)?;
    let root = crate::services::provider_runtime::ProviderHubPaths::for_home(home_root)
        .ensure_canonical_root()?;
    let path = root.join("bindings.json");
    if !path.exists() {
        return Ok(RouteKind::Direct);
    }
    let repository =
        ProfileBindingRepository::new(VersionedFileStore::<ProfileBindingCollection>::new(
            path,
            InstallationLock::new(root.join("provider-hub.lock"), Duration::from_secs(2)),
            1,
            StoreOptions::default(),
        ));
    Ok(repository
        .load()?
        .value
        .bindings
        .into_iter()
        .find(|binding| binding.profile_id == profile_id)
        .map(|binding| binding.route_kind)
        .unwrap_or(RouteKind::Direct))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlanInput {
    pub profile_id: String,
    pub codex_home: PathBuf,
    pub route_kind: RouteKind,
    pub entry: LaunchEntry,
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexLaunchPlan {
    pub route_kind: RouteKind,
    pub shell_command: String,
}

#[derive(Debug, Clone)]
pub struct CodexLaunchPlanner {
    launcher_command: String,
}

impl CodexLaunchPlanner {
    pub fn new(launcher_command: String) -> Self {
        Self { launcher_command }
    }

    pub fn plan(&self, input: LaunchPlanInput) -> Result<CodexLaunchPlan> {
        validate_profile_id(&input.profile_id)?;
        validate_arg(&self.launcher_command)?;
        let cwd = input
            .cwd
            .as_ref()
            .map(|cwd| format!("cd {} && ", shell_quote(cwd.to_string_lossy().as_ref())))
            .unwrap_or_default();
        let shell_command = match input.entry {
            LaunchEntry::RelayHandoff { session_id, prompt } => {
                validate_args([session_id.as_str(), prompt.as_str()])?;
                let first = self.render_invocation(
                    input.route_kind,
                    &input.profile_id,
                    &input.codex_home,
                    &["exec", "resume", &session_id, &prompt],
                );
                let second = self.render_invocation(
                    input.route_kind,
                    &input.profile_id,
                    &input.codex_home,
                    &["resume", &session_id],
                );
                format!("{cwd}{first} && {second}")
            }
            entry => {
                let args = entry_args(entry);
                validate_args(args.iter().map(String::as_str))?;
                format!(
                    "{cwd}{}",
                    self.render_invocation_owned(
                        input.route_kind,
                        &input.profile_id,
                        &input.codex_home,
                        &args,
                    )
                )
            }
        };
        Ok(CodexLaunchPlan {
            route_kind: input.route_kind,
            shell_command,
        })
    }

    pub fn gateway_wrapper_script(&self, profile_id: &str) -> Result<String> {
        validate_profile_id(profile_id)?;
        validate_arg(&self.launcher_command)?;
        Ok(format!(
            "#!/usr/bin/env bash\nset -euo pipefail\nexport CODEX_HOME=\"$HOME/.codex-{}\"\nexec {} codex --profile {} -- \"$@\"\n",
            profile_id,
            shell_quote(&self.launcher_command),
            shell_quote(profile_id),
        ))
    }

    fn render_invocation_owned(
        &self,
        route_kind: RouteKind,
        profile_id: &str,
        codex_home: &std::path::Path,
        args: &[String],
    ) -> String {
        let borrowed = args.iter().map(String::as_str).collect::<Vec<_>>();
        self.render_invocation(route_kind, profile_id, codex_home, &borrowed)
    }

    fn render_invocation(
        &self,
        route_kind: RouteKind,
        profile_id: &str,
        codex_home: &std::path::Path,
        args: &[&str],
    ) -> String {
        let suffix = args
            .iter()
            .map(|arg| shell_arg(arg))
            .collect::<Vec<_>>()
            .join(" ");
        match route_kind {
            RouteKind::Gateway => format!(
                "{} codex --profile {} --{}{}",
                shell_quote(&self.launcher_command),
                shell_quote(profile_id),
                if suffix.is_empty() { "" } else { " " },
                suffix,
            ),
            RouteKind::Direct => format!(
                "CODEX_HOME={} codex{}{}",
                shell_quote(codex_home.to_string_lossy().as_ref()),
                if suffix.is_empty() { "" } else { " " },
                suffix,
            ),
        }
    }
}

fn entry_args(entry: LaunchEntry) -> Vec<String> {
    match entry {
        LaunchEntry::Normal { args }
        | LaunchEntry::Terminal { args }
        | LaunchEntry::CopyCommand { args } => args,
        LaunchEntry::Resume {
            session_id: Some(session_id),
        } => vec!["resume".into(), session_id],
        LaunchEntry::Resume { session_id: None } => {
            vec!["resume".into(), "--last".into(), "--all".into()]
        }
        LaunchEntry::ExecResume { session_id, prompt } => {
            vec!["exec".into(), "resume".into(), session_id, prompt]
        }
        LaunchEntry::RelayHandoff { .. } => unreachable!("handled before entry_args"),
    }
}

fn validate_profile_id(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(AppError::new(
            "CODEX_PROFILE_ID_INVALID",
            "Codex profile identifier is invalid",
        ));
    }
    Ok(())
}

fn validate_args<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<()> {
    for arg in args {
        validate_arg(arg)?;
    }
    Ok(())
}

fn validate_arg(value: &str) -> Result<()> {
    if value.contains('\0') || value.contains(['\n', '\r']) {
        return Err(AppError::new(
            "CODEX_LAUNCH_ARGUMENT_INVALID",
            "Codex launch argument contains a forbidden control character",
        ));
    }
    Ok(())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn shell_arg(value: &str) -> String {
    if !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/' | b':')
        })
    {
        value.to_owned()
    } else {
        shell_quote(value)
    }
}
