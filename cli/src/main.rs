use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use reminder_core::{auth, bootstrap, notify, reminders, session_store};
use serde_json::Value;

#[derive(Parser)]
#[command(name = "reminder-proxy-client")]
struct Cli {
    /// Your Apple ID email.
    // clap forbids a `global` arg from also being `required` (a global arg
    // must be optional at every subcommand), so this is validated manually
    // right after parsing instead.
    #[arg(long, global = true)]
    apple_id: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Test the idmsa login flow end-to-end against the real Apple servers.
    Login,
    /// List all reminder lists.
    Lists,
    /// List reminders in one list.
    ListReminders {
        list_id: String,
        #[arg(long)]
        include_completed: bool,
    },
    /// Create a test reminder in a list.
    Create {
        list_id: String,
        title: String,
    },
    /// Set a reminder's priority (0 none, 1 high, 5 medium, 9 low) and flag.
    SetPriority {
        reminder_id: String,
        priority: i64,
        #[arg(long)]
        flagged: bool,
    },
    /// Move a reminder to a different list.
    Move {
        reminder_id: String,
        target_list_id: String,
    },
    /// Rewrite a list's manual sort order.
    Reorder {
        list_id: String,
        /// New order, e.g. "Reminder/AAA Reminder/BBB Reminder/CCC"
        #[arg(num_args = 1..)]
        reminder_ids: Vec<String>,
    },
    /// Soft-delete a reminder.
    Delete { reminder_id: String },
    /// Fire a test Windows toast notification. Does not touch any iCloud
    /// data or require login -- purely local.
    TestNotify,
    /// Poll for due reminders and fire Windows toast notifications.
    /// Only works while this process is running -- Apple exposes no push
    /// mechanism for Reminders, so there is no way to wake up otherwise.
    Watch {
        /// Poll interval in seconds. Every poll re-fetches all lists and all
        /// reminders in each list (no incremental sync yet), so keep this
        /// generous to avoid hammering the API / risking rate limits.
        #[arg(long, default_value_t = 300)]
        interval_secs: u64,
    },
}

#[cfg(windows)]
fn enable_utf8_console() {
    use windows_sys::Win32::System::Console::{SetConsoleCP, SetConsoleOutputCP};
    const CP_UTF8: u32 = 65001;
    unsafe {
        SetConsoleCP(CP_UTF8);
        SetConsoleOutputCP(CP_UTF8);
    }
}

#[cfg(not(windows))]
fn enable_utf8_console() {}

#[tokio::main]
async fn main() -> Result<()> {
    enable_utf8_console();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let cli = Cli::parse();

    if let Commands::TestNotify = cli.command {
        notify::send("reminder-proxy-client", "テスト通知です。これが見えれば通知機構は正常です。")?;
        println!("通知を送信しました。");
        return Ok(());
    }

    let apple_id = cli
        .apple_id
        .ok_or_else(|| anyhow!("--apple-id is required for this command"))?;

    if let Commands::Login = cli.command {
        login_test(&apple_id).await?;
        return Ok(());
    }

    let (http, client_id, account_data) = ensure_login(&apple_id).await?;
    let service_root = bootstrap::reminders_service_root(&account_data)?;
    let reminders = reminders::RemindersService::new(http, &service_root, &client_id);

    match cli.command {
        Commands::Login | Commands::TestNotify => unreachable!(),
        Commands::Lists => {
            let lists = reminders.lists().await?;
            for l in &lists {
                println!(
                    "{} | id={} | {}件 | order={:?}",
                    l.title,
                    l.id,
                    l.reminder_ids.len(),
                    l.reminder_ids
                );
            }
        }
        Commands::ListReminders {
            list_id,
            include_completed,
        } => {
            let items = reminders.list_reminders(&list_id, include_completed).await?;
            for r in &items {
                println!(
                    "{} | id={} | priority={} flagged={} completed={} due={:?}",
                    r.title, r.id, r.priority, r.flagged, r.completed, r.due_date
                );
            }
        }
        Commands::Create { list_id, title } => {
            let created = reminders
                .create(&list_id, &title, "", 0, false, None)
                .await?;
            println!("作成成功: id={}", created.id);
        }
        Commands::SetPriority {
            reminder_id,
            priority,
            flagged,
        } => {
            let mut r = reminders.get(&reminder_id).await?;
            r.priority = priority;
            r.flagged = flagged;
            let updated = reminders.update(&r).await?;
            println!(
                "更新後: priority={} flagged={}",
                updated.priority, updated.flagged
            );
        }
        Commands::Move {
            reminder_id,
            target_list_id,
        } => {
            let mut r = reminders.get(&reminder_id).await?;
            r.list_id = target_list_id;
            let updated = reminders.update(&r).await?;
            println!("移動後のlist_id: {}", updated.list_id);
        }
        Commands::Reorder {
            list_id,
            reminder_ids,
        } => {
            let lists = reminders.lists().await?;
            let list = lists
                .into_iter()
                .find(|l| l.id == list_id)
                .ok_or_else(|| anyhow!("list not found: {list_id}"))?;
            reminders.reorder(&list, &reminder_ids).await?;
            println!("並べ替え完了。");
        }
        Commands::Delete { reminder_id } => {
            let r = reminders.get(&reminder_id).await?;
            reminders.delete(&r).await?;
            println!("削除完了。");
        }
        Commands::Watch { interval_secs } => {
            watch_loop(&reminders, interval_secs).await?;
        }
    }

    Ok(())
}

async fn watch_loop(reminders: &reminders::RemindersService, interval_secs: u64) -> Result<()> {
    let mut notified: std::collections::HashSet<String> = std::collections::HashSet::new();
    println!("監視を開始します(間隔: {interval_secs}秒)。Ctrl+Cで終了。");
    loop {
        match check_due_reminders(reminders, &mut notified).await {
            Ok(0) => {}
            Ok(count) => println!("{count}件の通知を送信しました。"),
            Err(e) => eprintln!("[警告] ポーリング中にエラー: {e}"),
        }
        tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
    }
}

async fn check_due_reminders(
    reminders: &reminders::RemindersService,
    notified: &mut std::collections::HashSet<String>,
) -> Result<usize> {
    let lists = reminders.lists().await?;
    let now = chrono::Utc::now();
    let mut count = 0;

    for list in &lists {
        let items = reminders.list_reminders(&list.id, false).await?;
        for r in items {
            if r.completed {
                continue;
            }
            let Some(due) = r.due_date else { continue };
            if due <= now && !notified.contains(&r.id) {
                notify::send(&r.title, &format!("{} - 期限: {}", list.title, due.format("%Y-%m-%d %H:%M")))?;
                notified.insert(r.id.clone());
                count += 1;
            }
        }
    }
    Ok(count)
}

/// Log in (resuming a persisted session if possible) and return an
/// authenticated HTTP client plus the `accountLogin` response data.
async fn ensure_login(apple_id: &str) -> Result<(reqwest::Client, String, Value)> {
    if let Some(resumed) = bootstrap::try_resume_session(apple_id).await? {
        return Ok(resumed);
    }

    let dir = session_store::data_dir()?;
    let mut client = auth::AppleAuthClient::new(apple_id)?;

    let keyring_entry = keyring::Entry::new(bootstrap::KEYRING_SERVICE, apple_id)?;
    let (password, from_keyring) = match keyring_entry.get_password() {
        Ok(p) => (p, true),
        Err(_) => (
            rpassword::prompt_password(format!("{apple_id} のパスワード: "))?,
            false,
        ),
    };

    let outcome = client.login(&password).await?;
    let data = match outcome {
        auth::LoginOutcome::Complete(data) => data,
        auth::LoginOutcome::TwoFactorRequired => {
            println!("2FAコードが必要です(信頼済みデバイスに送信されています)。");
            let code = rpassword::prompt_password("2FAコードを入力: ")?;
            client.validate_trusted_device_code(&code).await?;
            client.trust_session().await?
        }
    };

    if !from_keyring
        && let Err(e) = keyring_entry.set_password(&password)
    {
        eprintln!("[警告] パスワードをキーリングに保存できませんでした: {e}");
    }

    bootstrap::persist_state(&client, &dir)?;
    Ok((client.http_client(), client.client_id().to_string(), data))
}

async fn login_test(apple_id: &str) -> Result<()> {
    let (_, _, data) = ensure_login(apple_id).await?;
    println!("ログイン成功。");
    print_webservices(&data);
    Ok(())
}

fn print_webservices(data: &Value) {
    if let Some(ws) = data.get("webservices").and_then(|v| v.as_object()) {
        println!("利用可能なwebservices ({}件):", ws.len());
        for key in ws.keys() {
            println!("  - {key}");
        }
    } else {
        println!("[警告] webservicesが応答に含まれていません: {data}");
    }
}
