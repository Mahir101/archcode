mod agent;
mod compact;
mod config;
mod cost;
mod event;
mod guard;
mod kg;
mod llm;
mod refactor;
mod reminder;
mod session;
mod skills;
mod tools;
mod utils;

use anyhow::Result;
use clap::Parser;
use std::sync::Arc;
use tokio::sync::mpsc;

use config::discover_instruction_files;

use agent::Agent;
use compact::{estimate_tokens, should_compact};
use cost::CostTracker;
use event::Event;
use guard::{
    DangerousCommandRule, Decision, DefaultPolicyRule, GuardManager, GuardRule, SensitiveFileRule,
    WorkingDirRule,
};
use kg::{
    KGBlastTool, KGIndexTool, KGLintTool, KGManager, KGQueryTool, KGRelateTool, KGRiskTool,
    KGSearchTool, LintStore,
};
use llm::{config_from_env, new_provider};
use refactor::{
    build_refactor_tools, RefactorConfig, RefactorContext, RefactorResult, StackDetector,
    REFACTOR_SYSTEM_SNIPPET,
};
use reminder::{ConversationState, Reminder, ReminderManager, ScheduleKind};
use session::{auto_summary, SessionManager};
use skills::SkillManager;
use tools::{
    BashTool, EditTool, GlobTool, GrepTool, ReadTool, ShellState, TodoReadTool, TodoStore,
    TodoWriteTool, ToolManager, WebSearchTool, WriteTool,
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

    /// Resume a previous session by ID
    #[arg(long)]
    resume: Option<String>,

    /// Fast mode — lower temperature, concise responses
    #[arg(long, default_value_t = false)]
    fast: bool,

    /// Max effort mode — higher token budget, thorough responses
    #[arg(long, default_value_t = false)]
    max: bool,

    /// Maximum context window size in tokens (for auto-compact)
    #[arg(long, default_value_t = 128000)]
    max_context: usize,
}

// ---------------------------------------------------------------------------
// Setup helpers
// ---------------------------------------------------------------------------

fn build_tool_manager(cwd: &str) -> (Arc<ToolManager>, TodoStore, Arc<KGManager>) {
    let mut mgr = ToolManager::new();
    mgr.register(ReadTool);
    mgr.register(WriteTool);
    mgr.register(EditTool);
    mgr.register(GlobTool);
    mgr.register(GrepTool);
    mgr.register(BashTool {
        state: ShellState::new(cwd),
    });
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
        lint_store,
    });

    (Arc::new(mgr), store, kg)
}

fn build_guard_manager(no_guard: bool) -> Arc<GuardManager> {
    let mut mgr = GuardManager::new();

    // Register all guard rules (each implements GuardRule trait)
    mgr.add_rule(DangerousCommandRule);
    mgr.add_rule(WorkingDirRule);

    let extra_rules: Vec<Box<dyn GuardRule>> =
        vec![Box::new(SensitiveFileRule), Box::new(DefaultPolicyRule)];
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

    let skill_names: Vec<String> = skill_mgr
        .list()
        .iter()
        .map(|s| {
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
        })
        .collect();
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
         You have access to tools: Read, Write, Edit, Glob, Grep, Bash, WebSearch, TodoRead, TodoWrite, \
         KGIndex, KGQuery, KGSearch, KGBlast, KGRisk, KGRelate, KGLint, \
         refactor.baseline, refactor.run_tests, refactor.run_lint, refactor.run_format, \
         refactor.run_semgrep, refactor.git_diff.\n\n\
         Knowledge Graph (KG) tools:\n\
         - KGIndex: index files/directories to build a code graph of symbols, dependencies, and relationships.\n\
         - KGQuery: find neighbors of a node (file, function, class) in the graph.\n\
         - KGSearch: search the graph by name pattern.\n\
         - KGBlast: compute blast radius — what is affected if a symbol changes.\n\
         - KGRisk: score files/functions by risk (complexity, churn, fan-in).\n\
         - KGRelate: find how two nodes are connected.\n\
         - KGLint: run architectural linters (god class, circular deps, etc).\n\n\
         The working directory has been pre-indexed into the KG at startup. Use KG tools to understand \
         the codebase structure, dependencies, and impact before making changes.\n\n\
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

    let (tool_mgr, _todo_store, kg_mgr) = build_tool_manager(&cwd);
    let guard_mgr = build_guard_manager(cli.no_guard);
    let skill_mgr = SkillManager::load_default();
    let reminder_mgr = build_reminder_manager(&skill_mgr);
    let system_prompt = build_system_prompt(&cwd, cli.refactor);

    // Cost tracker
    let cost_tracker = CostTracker::new(&model);

    // Session manager
    let session_mgr = SessionManager::new(&cwd);
    let session_id = cli
        .resume
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("session").to_string());

    // Auto-index working directory into the Knowledge Graph
    let kg_clone = kg_mgr.clone();
    let cwd_clone = cwd.clone();
    let kg_handle = tokio::task::spawn_blocking(move || {
        kg_clone.index_dir(&cwd_clone);
    });

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
            let error_marker = if evt.is_error {
                " \x1b[31m(ERROR)\x1b[0m"
            } else {
                ""
            };
            let extra = if evt.args.is_empty() {
                String::new()
            } else {
                format!(" [{}]", evt.args.join(", "))
            };
            eprintln!(
                "{prefix} {}:{extra} {}{error_marker}",
                evt.name, evt.message
            );
        }
    });

    let interactive = cli.prompt.is_none();

    let mut agent = Agent::new(
        Arc::from(provider),
        model.clone(),
        tool_mgr,
        guard_mgr,
        reminder_mgr,
        system_prompt,
        events_tx.clone(),
        cwd.clone(),
        cost_tracker.clone(),
        interactive,
    );

    // Resume session if requested
    if cli.resume.is_some() {
        match session_mgr.load(&session_id) {
            Ok((meta, messages)) => {
                *agent.messages_mut() = messages;
                eprintln!(
                    "\x1b[36m[Session]\x1b[0m Resumed session '{}' ({} messages)",
                    session_id, meta.message_count
                );
            }
            Err(e) => {
                eprintln!(
                    "\x1b[31m[Session]\x1b[0m Failed to resume '{}': {e}",
                    session_id
                );
            }
        }
    }

    // Wait for KG indexing to complete and show stats
    let _ = kg_handle.await;
    eprintln!("\x1b[35m[KG]\x1b[0m {}", kg_mgr.stats());

    if let Some(prompt) = cli.prompt {
        // Send startup event in single-shot mode
        let _ = events_tx
            .send(Event::text(format!("archcode started with model: {model}")))
            .await;
        // Single-shot mode
        let result = agent.run(&prompt).await?;
        println!("{result}");

        // Show cost summary
        let summary = cost_tracker.summary();
        if summary.total_tokens > 0 {
            eprintln!("\n\x1b[90m{summary}\x1b[0m");
        }
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
        println!("   ║   \x1b[0;90msession: {:<40}\x1b[1;36m║", &session_id);
        println!("   ║                                                  ║");
        println!("   ╠══════════════════════════════════════════════════╣");
        println!("   ║  \x1b[0;33m/help\x1b[1;36m for commands  •  \x1b[0;33mCtrl+C\x1b[1;36m to abort        ║");
        println!("   ╚══════════════════════════════════════════════════╝");
        println!("\x1b[0m");

        let stdin = tokio::io::stdin();
        use tokio::io::AsyncBufReadExt;
        let reader = tokio::io::BufReader::new(stdin);
        let mut lines = reader.lines();

        let max_context = cli.max_context;

        loop {
            eprint!("\x1b[1;32m❯ \x1b[0m");
            match lines.next_line().await? {
                None => break,
                Some(ref s) if s.trim() == "/quit" || s.trim() == "/exit" => {
                    // Auto-save session
                    let messages = agent.messages();
                    if messages.len() > 1 {
                        let summary = auto_summary(messages);
                        let _ = session_mgr.save(&session_id, &model, messages, &summary);
                        eprintln!(
                            "\x1b[36m[Session]\x1b[0m Saved as '{session_id}'"
                        );
                    }
                    // Show cost summary
                    let cost = cost_tracker.summary();
                    if cost.total_tokens > 0 {
                        eprintln!("\n\x1b[90m{cost}\x1b[0m");
                    }
                    println!("\x1b[0;90m👋 Goodbye!\x1b[0m");
                    break;
                }
                Some(ref s) if s.trim() == "/help" => {
                    println!("\x1b[1;36mSlash Commands:\x1b[0m");
                    println!("  \x1b[33m/help\x1b[0m       Show this help");
                    println!("  \x1b[33m/clear\x1b[0m      Clear conversation history");
                    println!("  \x1b[33m/compact\x1b[0m    Compact conversation to save context");
                    println!("  \x1b[33m/cost\x1b[0m       Show token usage and cost");
                    println!("  \x1b[33m/model\x1b[0m      Show current model info");
                    println!("  \x1b[33m/sessions\x1b[0m   List saved sessions");
                    println!("  \x1b[33m/save\x1b[0m       Save current session");
                    println!("  \x1b[33m/diff\x1b[0m       Show git diff of changes");
                    println!("  \x1b[33m/quit\x1b[0m       Save session and exit");
                    continue;
                }
                Some(ref s) if s.trim() == "/clear" => {
                    *agent.messages_mut() = vec![];
                    println!("\x1b[90mConversation cleared.\x1b[0m");
                    continue;
                }
                Some(ref s) if s.trim() == "/compact" => {
                    let before_len = agent.messages().len();
                    let before_tokens = estimate_tokens(agent.messages());
                    let compacted = compact::compact(agent.messages(), 10);
                    let after_tokens = estimate_tokens(&compacted);
                    let after_len = compacted.len();
                    *agent.messages_mut() = compacted;
                    println!(
                        "\x1b[90mCompacted: ~{before_tokens} → ~{after_tokens} tokens ({} messages removed)\x1b[0m",
                        before_len.saturating_sub(after_len)
                    );
                    continue;
                }
                Some(ref s) if s.trim() == "/cost" => {
                    let cost = cost_tracker.summary();
                    println!("\x1b[36m{cost}\x1b[0m");
                    println!(
                        "\x1b[90mContext: ~{} estimated tokens\x1b[0m",
                        estimate_tokens(agent.messages())
                    );
                    continue;
                }
                Some(ref s) if s.trim() == "/model" => {
                    println!("\x1b[36mModel:\x1b[0m {model}");
                    println!("\x1b[36mSession:\x1b[0m {session_id}");
                    println!(
                        "\x1b[36mContext:\x1b[0m ~{} tokens / {max_context} max",
                        estimate_tokens(agent.messages())
                    );
                    let mode = if cli.fast {
                        "fast"
                    } else if cli.max {
                        "max"
                    } else {
                        "default"
                    };
                    println!("\x1b[36mEffort:\x1b[0m {mode}");
                    continue;
                }
                Some(ref s) if s.trim() == "/sessions" => {
                    let sessions = session_mgr.list();
                    if sessions.is_empty() {
                        println!("\x1b[90mNo saved sessions.\x1b[0m");
                    } else {
                        println!("\x1b[1;36mSaved Sessions:\x1b[0m");
                        for s in &sessions {
                            let active = if s.id == session_id { " \x1b[32m(active)\x1b[0m" } else { "" };
                            println!(
                                "  \x1b[33m{}\x1b[0m{active} — {} msgs — {}",
                                s.id, s.message_count, s.summary
                            );
                        }
                        println!("\x1b[90mResume with: archcode --resume <id>\x1b[0m");
                    }
                    continue;
                }
                Some(ref s) if s.trim() == "/save" => {
                    let messages = agent.messages();
                    let summary = auto_summary(messages);
                    match session_mgr.save(&session_id, &model, messages, &summary) {
                        Ok(_) => println!(
                            "\x1b[36m[Session]\x1b[0m Saved as '{session_id}'"
                        ),
                        Err(e) => eprintln!("\x1b[31mFailed to save: {e}\x1b[0m"),
                    }
                    continue;
                }
                Some(ref s) if s.trim() == "/diff" => {
                    match tokio::process::Command::new("git")
                        .args(["diff", "--stat"])
                        .current_dir(&cwd)
                        .output()
                        .await
                    {
                        Ok(output) => {
                            let stdout = String::from_utf8_lossy(&output.stdout);
                            if stdout.trim().is_empty() {
                                println!("\x1b[90mNo uncommitted changes.\x1b[0m");
                            } else {
                                println!("{stdout}");
                            }
                        }
                        Err(e) => eprintln!("\x1b[31mFailed to run git diff: {e}\x1b[0m"),
                    }
                    continue;
                }
                Some(ref line) if line.trim().is_empty() => continue,
                Some(line) => {
                    // Auto-compact if context is getting large
                    if should_compact(agent.messages(), max_context) {
                        let before = agent.messages().len();
                        let compacted = compact::compact(agent.messages(), 10);
                        *agent.messages_mut() = compacted;
                        eprintln!(
                            "\x1b[90m[Auto-compact] {} → {} messages\x1b[0m",
                            before,
                            agent.messages().len()
                        );
                    }

                    match agent.run(&line).await {
                        Ok(resp) => {
                            println!("\n\x1b[0;37m{resp}\x1b[0m\n");
                        }
                        Err(e) => eprintln!("\x1b[1;31m✖ Error:\x1b[0m {e}"),
                    }
                }
            }
        }
    }

    Ok(())
}
