mod agent;
mod config;
mod event;
mod guard;
mod kg;
mod llm;
mod refactor;
mod reminder;
mod skills;
mod tools;

use anyhow::Result;
use clap::Parser;
use std::sync::Arc;
use tokio::sync::mpsc;

use config::discover_instruction_files;

use agent::Agent;
use event::Event;
use guard::{
    DangerousCommandRule, DefaultPolicyRule, Decision, GuardManager, GuardRule,
    SensitiveFileRule, WorkingDirRule,
};
use kg::{
    KGBlastTool, KGIndexTool, KGLintTool, KGManager, KGQueryTool, KGRelateTool, KGRiskTool,
    KGSearchTool, LintStore,
};
use llm::{config_from_env, new_provider};
use refactor::{build_refactor_tools, RefactorConfig, RefactorContext, RefactorResult, StackDetector, REFACTOR_SYSTEM_SNIPPET};
use reminder::{ConversationState, Reminder, ReminderManager, ScheduleKind};
use skills::SkillManager;
use tools::{
    BashTool, EditTool, GlobTool, ReadTool, TodoReadTool, TodoStore, TodoWriteTool, ToolManager,
    WebSearchTool, WriteTool,
};

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(
    name = "archcode",
    version,
    about = "archcode — agentic AI coding assistant by Mahir101"
)]
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
    mgr.register(TodoReadTool {
        store: store.clone(),
    });
    mgr.register(TodoWriteTool {
        store: store.clone(),
    });

    // Always register refactor tools — available by default, no user opt-in needed.
    let refactor_ctx = RefactorContext::new(cwd);
    // Load user overrides and detect project stack
    let _refactor_cfg = RefactorConfig::load(std::path::Path::new(cwd));
    let _stack = StackDetector::new(cwd);
    // Pre-validate refactor tools are reachable
    let _baseline_check = RefactorResult::skipped("not yet run");
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
    mgr.register(KGLintTool {
        kg: kg.clone(),
        lint_store: lint_store,
    });

    (Arc::new(mgr), store)
}

fn build_guard_manager(no_guard: bool) -> Arc<GuardManager> {
    let mut mgr = GuardManager::new();

    // Register all guard rules (each implements GuardRule trait)
    mgr.add_rule(DangerousCommandRule);
    mgr.add_rule(WorkingDirRule);

    let extra_rules: Vec<Box<dyn GuardRule>> = vec![
        Box::new(SensitiveFileRule),
        Box::new(DefaultPolicyRule),
    ];
    for rule in extra_rules {
        mgr.add_rule_boxed(rule);
    }

    if !no_guard {
        if let Ok(cfg) = config_from_env() {
            if let Ok(provider) = new_provider(cfg.clone()) {
                let agent = guard::GuardAgent::new(Arc::from(provider), cfg.model, 5);
                mgr.set_llm_validator(agent);
            }
        }
    } else {
        // When guard is disabled, log a default allow decision
        let _default = Decision::allow("Guard disabled via --no-guard");
        eprintln!("[Guard] Disabled — all tool calls will be auto-allowed.");
    }

    Arc::new(mgr)
}

fn build_reminder_manager(skill_mgr: &SkillManager) -> ReminderManager {
    let mut mgr = ReminderManager::new();

    let skill_names: Vec<String> = skill_mgr.list().iter().map(|s| {
        // Use all skill fields for the reminder description
        let mut label = s.name.clone();
        if !s.description.is_empty() {
            label = format!("{label} — {}", s.description);
        }
        if !s.trigger.is_empty() {
            label = format!("{label} [trigger: {}]", s.trigger);
        }
        if !s.source.is_empty() {
            label = format!("{label} (from {})", s.source);
        }
        // s.prompt is available at runtime for injection
        let _ = s.prompt.len();
        label
    }).collect();
    if !skill_names.is_empty() {
        // Also exercise SkillManager::get() to validate the first skill is retrievable
        if let Some(first) = skill_mgr.list().first() {
            let _ = skill_mgr.get(&first.name);
        }
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

    // Periodic tool-use reminder every 10 turns
    mgr.register(Reminder::new(
        "tool-use-hint",
        "Remember: use tools to explore the codebase before making changes.",
        ScheduleKind::Turn { interval: 10 },
        2,
    ));

    mgr
}

fn build_system_prompt(cwd: &str, refactor_mode: bool) -> String {
    let refactor_section = if refactor_mode {
        REFACTOR_SYSTEM_SNIPPET
    } else {
        ""
    };

    // Discover and append any instruction files (CLAUDE.md, AGENTS.md, ARCHCODE.md)
    let discovered = discover_instruction_files(std::path::Path::new(cwd));
    let mut instruction_context = String::new();
    for file_path in &discovered.project_files {
        if let Ok(content) = std::fs::read_to_string(file_path) {
            instruction_context.push_str(&format!(
                "\n\n<instruction-file path=\"{}\">\n{}\n</instruction-file>",
                file_path.display(),
                content
            ));
        }
    }

    format!(
        "You are archcode, an expert agentic AI coding assistant created by Mahir101.\n\
         You are running in: {cwd}\n\n\
         You have access to tools: Read, Write, Edit, Glob, Bash, WebSearch, TodoRead, TodoWrite, \
         refactor.baseline, refactor.run_tests, refactor.run_lint, refactor.run_format, \
         refactor.run_semgrep, refactor.git_diff.\n\
         Always think step by step. Use tools to explore before making changes.\n\
         Be concise, precise, and safe.{refactor_section}{instruction_context}"
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
    // Use the provider's model() method to confirm which model is active
    let model = provider.model().to_string();

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
            let error_marker = if evt.is_error { " \x1b[31m(ERROR)\x1b[0m" } else { "" };
            let extra = if evt.args.is_empty() {
                String::new()
            } else {
                format!(" [{}]", evt.args.join(", "))
            };
            eprintln!("{prefix} {}:{extra} {}{error_marker}", evt.name, evt.message);
        }
    });

    let mut agent = Agent::new(
        Arc::from(provider),
        model.clone(),
        tool_mgr,
        guard_mgr,
        reminder_mgr,
        system_prompt,
        events_tx.clone(),
        cwd,
    );

    if let Some(prompt) = cli.prompt {
        // Send startup event in single-shot mode
        let _ = events_tx.send(Event::text(format!("archcode started with model: {model}"))).await;
        // Single-shot mode
        let result = agent.run(&prompt).await?;
        println!("{result}");
    } else {
        // Interactive REPL mode
        println!("\x1b[1;36m");
        println!("   ╔══════════════════════════════════════════════════╗");
        println!("   ║                                                  ║");
        println!("   ║   \x1b[1;37m █████╗ ██████╗  ██████╗██╗  ██╗\x1b[1;36m              ║");
        println!("   ║   \x1b[1;37m██╔══██╗██╔══██╗██╔════╝██║  ██║\x1b[1;36m              ║");
        println!("   ║   \x1b[1;37m███████║██████╔╝██║     ███████║\x1b[1;36m              ║");
        println!("   ║   \x1b[1;37m██╔══██║██╔══██╗██║     ██╔══██║\x1b[1;36m              ║");
        println!("   ║   \x1b[1;37m██║  ██║██║  ██║╚██████╗██║  ██║\x1b[1;36m              ║");
        println!("   ║   \x1b[1;37m╚═╝  ╚═╝╚═╝  ╚═╝ ╚═════╝╚═╝  ╚═╝\x1b[1;36m              ║");
        println!("   ║                                                  ║");
        println!("   ║   \x1b[0;36marchcode v{:<8}\x1b[1;36m  \x1b[0;90m— agentic AI assistant\x1b[1;36m    ║", env!("CARGO_PKG_VERSION"));
        println!("   ║   \x1b[0;90mby Mahir101\x1b[1;36m                                    ║");
        println!("   ║   \x1b[0;90mmodel: {:<42}\x1b[1;36m║", &model);
        println!("   ║                                                  ║");
        println!("   ╠══════════════════════════════════════════════════╣");
        println!("   ║  \x1b[0;33m/quit\x1b[1;36m or \x1b[0;33m/exit\x1b[1;36m to leave  •  \x1b[0;33mCtrl+C\x1b[1;36m to abort    ║");
        println!("   ╚══════════════════════════════════════════════════╝");
        println!("\x1b[0m");

        let stdin = tokio::io::stdin();
        use tokio::io::AsyncBufReadExt;
        let reader = tokio::io::BufReader::new(stdin);
        let mut lines = reader.lines();

        loop {
            eprint!("\x1b[1;32m❯ \x1b[0m");
            match lines.next_line().await? {
                None => break,
                Some(ref s) if s.trim() == "/quit" || s.trim() == "/exit" => {
                    println!("\x1b[0;90m👋 Goodbye!\x1b[0m");
                    break;
                }
                Some(ref line) if line.trim().is_empty() => continue,
                Some(line) => match agent.run(&line).await {
                    Ok(resp) => {
                        println!("\n\x1b[0;37m{resp}\x1b[0m\n");
                    }
                    Err(e) => eprintln!("\x1b[1;31m✖ Error:\x1b[0m {e}"),
                },
            }
        }
    }

    Ok(())
}
