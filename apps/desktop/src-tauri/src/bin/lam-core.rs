use localagentmanager_core::{list_accounts, resolve_home_root};

fn main() {
    let home = resolve_home_root().expect("HOME is required");
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
