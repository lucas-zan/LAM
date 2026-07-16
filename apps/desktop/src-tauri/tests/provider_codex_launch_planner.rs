use localagentmanager_core::gateway::launch_planner::{
    CodexLaunchPlanner, LaunchEntry, LaunchPlanInput,
};
use localagentmanager_core::provider_binding::RouteKind;
use std::path::PathBuf;

fn input(route_kind: RouteKind, entry: LaunchEntry) -> LaunchPlanInput {
    LaunchPlanInput {
        profile_id: "profile-a".into(),
        codex_home: PathBuf::from("/tmp/profile home"),
        route_kind,
        entry,
        cwd: Some(PathBuf::from("/tmp/work dir")),
    }
}

#[test]
fn gateway_matrix_routes_every_supported_entry_through_lam_launcher() {
    let planner = CodexLaunchPlanner::new("/Applications/LAM.app/Contents/MacOS/lam".into());
    let entries = [
        LaunchEntry::Normal {
            args: vec!["--model".into(), "deepseek-chat".into()],
        },
        LaunchEntry::Resume {
            session_id: Some("session 1".into()),
        },
        LaunchEntry::Resume { session_id: None },
        LaunchEntry::ExecResume {
            session_id: "session 1".into(),
            prompt: "summarize 'this'".into(),
        },
        LaunchEntry::Terminal {
            args: vec!["resume".into(), "--last".into(), "--all".into()],
        },
        LaunchEntry::CopyCommand {
            args: vec!["resume".into(), "session 1".into()],
        },
    ];
    for entry in entries {
        let plan = planner.plan(input(RouteKind::Gateway, entry)).unwrap();
        assert!(plan
            .shell_command
            .contains("'/Applications/LAM.app/Contents/MacOS/lam' codex --profile 'profile-a' --"));
        assert!(!plan.shell_command.contains("CODEX_HOME="));
        assert_eq!(plan.route_kind, RouteKind::Gateway);
    }
}

#[test]
fn direct_entries_use_same_planner_and_preserve_existing_command_shape() {
    let planner = CodexLaunchPlanner::new("lam".into());
    let resume = planner
        .plan(input(
            RouteKind::Direct,
            LaunchEntry::Resume {
                session_id: Some("session 1".into()),
            },
        ))
        .unwrap();
    assert!(resume
        .shell_command
        .contains("CODEX_HOME='/tmp/profile home' codex resume 'session 1'"));
    let last = planner
        .plan(input(
            RouteKind::Direct,
            LaunchEntry::Resume { session_id: None },
        ))
        .unwrap();
    assert!(last.shell_command.contains("codex resume --last --all"));
    assert!(!last.shell_command.contains("lam codex"));
}

#[test]
fn relay_handoff_sequence_preserves_cwd_session_prompt_and_safe_escaping() {
    let planner = CodexLaunchPlanner::new("lam".into());
    let plan = planner
        .plan(input(
            RouteKind::Gateway,
            LaunchEntry::RelayHandoff {
                session_id: "session; rm -rf /".into(),
                prompt: "read $(touch /tmp/nope) and 'summarize'".into(),
            },
        ))
        .unwrap();
    assert_eq!(
        plan.shell_command.matches("lam' codex --profile").count(),
        2
    );
    assert!(plan.shell_command.contains("exec resume"));
    assert!(plan.shell_command.contains("&&"));
    assert!(!plan
        .shell_command
        .contains("touch /tmp/nope) and 'summarize'"));
}

#[test]
fn wrapper_script_never_constructs_a_raw_codex_bypass() {
    let planner =
        CodexLaunchPlanner::new("/Applications/LAM Preview.app/Contents/MacOS/lam".into());
    let wrapper = planner.wrapper_script("profile-a").unwrap();
    assert!(wrapper.contains(
        "exec '/Applications/LAM Preview.app/Contents/MacOS/lam' codex --profile 'profile-a' -- \"$@\""
    ));
    assert!(wrapper.contains("export CODEX_HOME=\"$HOME/.codex-profile-a\""));
    assert!(!wrapper.contains("exec \"$CODEX_BIN\""));
    assert!(!wrapper.contains("exec 'lam' codex"));
}

#[test]
fn wrapper_script_rejects_invalid_profile_ids() {
    let planner = CodexLaunchPlanner::new("/Applications/LAM.app/Contents/MacOS/lam".into());
    let error = planner.wrapper_script("../main").unwrap_err();
    assert_eq!(error.code, "CODEX_PROFILE_ID_INVALID");
}
