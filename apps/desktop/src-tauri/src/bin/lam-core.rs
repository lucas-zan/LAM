use localagentmanager_core::list_accounts;
use std::path::PathBuf;

fn main() {
    let home = std::env::var("LAM_HOME")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(PathBuf::from))
        .expect("HOME is required");
    match list_accounts(&home) {
        Ok(accounts) => {
            println!("LocalAgentManager core");
            println!("accounts={}", accounts.len());
            for account in accounts {
                println!(
                    "{}\t{}\tsessions={}\tmanaged={}\trelay={}",
                    account.id,
                    account.codex_home.display(),
                    account.session_count,
                    account.managed,
                    account.is_relay
                );
            }
        }
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }
}
