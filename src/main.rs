mod agent;
mod event;
mod guard;
mod kg;
mod llm;
mod refactor;
mod reminder;
mod skills;
mod tools;
mod config;

use std::sync::Arc;
use anyhow::Result;
use clap::Parser;
use tokio::sync::mpsc;

use agent::Agent;
use event::Event;
use guard::{
    DangerousCommandRule, DefaultPolicyRule, GuardManager, SensitiveFileRule, WorkingDirRule,
};
use kg::{KGManager, KGIndexTool, KGQueryTool, KGSearchTool, KGBlastTool, KGRiskTool, KGRelateTool, KGLintTool, LintStore};
use llm::{config_from_env, new_provider};
use reminder::{ReminderManager, Reminder, ScheduleKind, ConversationState};
use skills::SkillManager;
use refactor::{RefactorContext, build_refactor_tools, REFACTOR_SYSTEM_SNIPPET};
use tools::{
    BashTool, EditTool, GlobTool, ReadTool, ToolManager, TodoReadTool, TodoStore, TodoWriteTool,
    WebSearchTool, WriteTool,
};

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(name = "rapcode", version, about = "rapcode — agentic AI coding assistant by Mahir101")]
struct Cli {
    /// Single-shot prompt (non-interactive)
    #[arg(short, long)]
    prompt: Option<String>,

    /// Disable the guard agent
    #[arg(long, default_value_t = false)]
    no_guard: bool,

    /// Enable SOLID Refactoring Mode — injects playbook rules into the system prompt
    /// and makes all refactor.* tools available to the agent.
    #[arg(long, default_value_t = false)]
    refactor: bool,
}

// ---------------------------------------------------------------------------
// Setup helpers
// ---------------------------------------------------------------------------

fn build_tool_manager(cwd: &str) -> (Arc<ToolManager>, TodoStore) {
    let mut mgr = ToolManager::new();
    mgr.register(ReadTool);
    mgr.register(WriteTool);
    mgr.register(EditTool);
    mgr.register(GlobTool);
    mgr.register(BashTool);
    mgr.register(WebSearchTool);

    let store = TodoStore::new();
    mgr.register(TodoReadTool { store: store.clone() });
    mgr.register(TodoWriteTool { store: store.clone() });

    // Always register refactor tools — available by default, no user opt-in needed.
    let refactor_ctx = RefactorContext::new(cwd);
    for tool in build_refactor_tools(refactor_ctx) {
        mgr.register_boxed(tool);
    }

    // Register KG tools backed by a shared KGManager + LintStore.
    let kg = Arc::new(KGManager::new());
    let lint_store = Arc::new(std::sync::Mutex::new(LintStore::new()));
    mgr.register(KGIndexTool { kg: kg.clone() });
    mgr.register(KGQueryTool { kg: kg.clone() });
    mgr.register(KGSearchTool { kg: kg.clone() });
    mgr.register(KGBlastTool { kg: kg.clone() });
    mgr.register(KGRiskTool { kg: kg.clone() });
    mgr.register(KGRelateTool { kg: kg.clone() });
    mgr.register(KGLintTool { kg: kg.clone(), lint_store: lint_store });

    (Arc::new(mgr), store)
}

fn build_guard_manager(no_guard: bool) -> Arc<GuardManager> {
    let mut mgr = GuardManager::new();
    mgr.add_rule(DangerousCommandRule);
    mgr.add_rule(WorkingDirRule);
    mgr.add_rule(SensitiveFileRule);
    mgr.add_rule(DefaultPolicyRule);

    if !no_guard {
        if let Ok(cfg) = config_from_env() {
            if let Ok(provider) = new_provider(cfg.clone()) {
                let agent = guard::GuardAgent::new(
                    Arc::from(provider),
                    cfg.model,
                    5,
                );
                mgr.set_llm_validator(agent);
            }
        }
    }

    Arc::new(mgr)
}

fn build_reminder_manager(skill_mgr: &SkillManager) -> ReminderManager {
    let mut mgr = ReminderManager::new();

    let skill_names: Vec<String> = skill_mgr.list().iter().map(|s| s.name.clone()).collect();
    if !skill_names.is_empty() {
        mgr.register(Reminder::new(
            "skill-availability",
            format!("Available skills: {}", skill_names.join(", ")),
            ScheduleKind::OneShot,
            0,
        ));
    }

    mgr.register(Reminder::new(
        "conversation-length",
        "The conversation is getting long. Consider summarizing context.",
        ScheduleKind::Condition {
            max_fires: 2,
            condition: Arc::new(|s: &ConversationState| s.message_count > 80),
        },
        1,
    ));

    mgr
}

fn build_system_prompt(cwd: &str, refactor_mode: bool) -> String {
    let refactor_section = if refactor_mode { REFACTOR_SYSTEM_SNIPPET } else { "" };
    format!(
        "You are rapcode, an expert agentic AI coding assistant created by Mahir101.\n\
         You are running in: {cwd}\n\n\
         You have access to tools: Read, Write, Edit, Glob, Bash, WebSearch, TodoRead, TodoWrite, \
         refactor.baseline, refactor.run_tests, refactor.run_lint, refactor.run_format, \
         refactor.run_semgrep, refactor.git_diff.\n\
         Always think step by step. Use tools to explore before making changes.\n\
         Be concise, precise, and safe.{refactor_section}"
    )
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let cli = Cli::parse();

    let cfg = config_from_env()?;
    let provider = new_provider(cfg.clone())?;
    let model = cfg.model.clone();

    let cwd = std::env::current_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let (tool_mgr, _todo_store) = build_tool_manager(&cwd);
    let guard_mgr = build_guard_manager(cli.no_guard);
    let skill_mgr = SkillManager::load_default();
    let reminder_mgr = build_reminder_manager(&skill_mgr);
    let system_prompt = build_system_prompt(&cwd, cli.refactor);

    let (events_tx, mut events_rx) = mpsc::channel::<Event>(128);

    // Spawn event printer
    tokio::spawn(async move {
        while let Some(evt) = events_rx.recv().await {
            let prefix = match evt.preview_type {
                event::PreviewType::Guard => "\x1b[33m[Guard]\x1b[0m",
                event::PreviewType::Tool => "\x1b[36m[Tool]\x1b[0m",
                event::PreviewType::KG => "\x1b[35m[KG]\x1b[0m",
                event::PreviewType::Text => "\x1b[0m",
            };
            eprintln!("{prefix} {}: {}", evt.name, evt.message);
        }
    });

    let mut agent = Agent::new(
        Arc::from(provider),
        model,
        tool_mgr,
        guard_mgr,
        reminder_mgr,
        system_prompt,
        events_tx,
        cwd,
    );

    if let Some(prompt) = cli.prompt {
        // Single-shot mode
        let result = agent.run(&prompt).await?;
        println!("{result}");
    } else {
        // Interactive REPL mode
        println!("rapcode v{} — by Mahir101", env!("CARGO_PKG_VERSION"));
        println!("Type your prompt and press Enter. Ctrl+C to exit.\n");

        let stdin = tokio::io::stdin();
        use tokio::io::AsyncBufReadExt;
        let reader = tokio::io::BufReader::new(stdin);
        let mut lines = reader.lines();

        loop {
            eprint!("> ");
            match lines.next_line().await? {
                None => break,
                Some(ref s) if s.trim() == "/quit" || s.trim() == "/exit" => break,
                Some(ref line) if line.trim().is_empty() => continue,
                Some(line) => {
                    match agent.run(&line).await {
                        Ok(resp) => println!("\n{resp}\n"),
                        Err(e) => eprintln!("Error: {e}"),
                    }
                }
            }
        }
    }

    Ok(())
}
