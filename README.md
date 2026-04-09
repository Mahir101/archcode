# ArchCode

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](http://makeapullrequest.com)

**ArchCode** is an agentic AI coding assistant by Mahir101. Built in Rust, ArchCode combines safety, code understanding, and extensible tooling to explore, refactor, and change codebases with an AI-driven workflow.

ArchCode is designed to be used as a developer assistant for:
- analyzing repository structure,
- making safe code edits,
- performing targeted refactorings,
- running shell-based exploration,
- and persisting multi-turn sessions.

## Key Features

- **Agentic execution**: multi-turn interaction with tool-enabled AI responses.
- **Safety guards**: built-in rules and an optional LLM guard prevent dangerous operations.
- **Knowledge Graph (KG)**: local code graph indexing for relationships, risk, dependencies, and blast radius.
- **Session persistence**: save and resume conversations in `.archcode/sessions/`.
- **Token/cost tracking**: displays usage and estimated API cost in REPL sessions.
- **Persistent Bash shell**: maintains working directory and environment state across commands.
- **Slash commands**: fast interactive controls like `/help`, `/cost`, `/sessions`, `/compact`, `/diff`, and more.
- **Auto-compact**: compresses old history when context grows too large.
- **SOLID Refactoring Mode**: dedicated refactor prompts and toolset for cleaner code changes.

## Installation

Make sure Rust and Cargo are installed, then build the project:

```bash
cargo build --release
```

The binary will be available at:

```bash
./target/release/archcode
```

## Configuration

ArchCode uses environment variables to configure the LLM provider.
Supported variables:

- `ARCHCODE_MODEL` — model name (default: `gpt-4o`)
- `ARCHCODE_API_KEY` — API key for OpenAI-compatible providers
- `ARCHCODE_BASE_URL` — custom OpenAI-compatible endpoint
- `ARCHCODE_PROVIDER` — provider override (`openai` or `anthropic`)

Example `.env`:

```ini
ARCHCODE_MODEL=gpt-4o
ARCHCODE_API_KEY=sk-...
ARCHCODE_BASE_URL=https://api.openai.com/v1
ARCHCODE_PROVIDER=openai
```

## Usage

### Start interactive mode

```bash
./target/release/archcode
```

### Run a one-off prompt

```bash
./target/release/archcode --prompt "Refactor src/utils.rs to remove duplication"
```

### Resume a saved session

```bash
./target/release/archcode --resume <session_id>
```

## CLI Options

| Flag | Description |
|---|---|
| `--prompt <STRING>` | Run a single prompt and exit |
| `--no-guard` | Disable the safety guard agent |
| `--refactor` | Enable SOLID Refactoring Mode |
| `--resume <ID>` | Resume a previously saved session |
| `--fast` | Low-temperature fast responses |
| `--max` | Higher-effort, more thorough responses |
| `--max-context <TOKENS>` | Adjust the auto-compact trigger threshold (default: 128000) |

Example:

```bash
./target/release/archcode --refactor --fast
```

## Interactive Slash Commands

Use REPL commands to control the session without leaving the chat.

- `/help` — show available commands
- `/clear` — clear current conversation history
- `/compact` — manually compact conversation context
- `/cost` — show token usage and estimated cost
- `/model` — show active model, session ID, and context state
- `/sessions` — list saved sessions
- `/save` — save the current session immediately
- `/diff` — show current git diff summary
- `/quit` or `/exit` — save and exit

## Session Management

Sessions are stored in `.archcode/sessions/`.
When you exit interactive mode, ArchCode auto-saves the current session if there is history.
Use `--resume <session_id>` to continue later.

## Tool Overview

ArchCode includes an extensible toolset for repository exploration and editing.

- `Read`: read file contents
- `Write`: write or create files
- `Edit`: perform exact text replacement edits
- `Glob`: expand filename patterns
- `Grep`: search workspace text with ripgrep or grep fallback
- `Bash`: execute shell commands in a persistent shell session
- `WebSearch`: search the web from within the REPL
- `TodoRead` / `TodoWrite`: manage persistent todo state
- `KGIndex`, `KGQuery`, `KGSearch`, `KGBlast`, `KGRisk`, `KGRelate`, `KGLint`: knowledge graph exploration and risk analysis
- `refactor.*`: refactor-specific tools for tests, linting, formatting, semantic patterns, and diff review

### Persistent Bash Shell

The `Bash` tool preserves working directory and exported environment variables across commands, enabling a more natural shell-like workflow.

## Knowledge Graph (KG)

At startup, ArchCode automatically indexes the current repository into a local KG. This graph supports:

- symbol search and relationships
- blast radius analysis
- dependency tracing
- risk scoring
- structural queries

Use KG tools before making edits to understand change impact and reduce risk.

## Safety & Guarding

ArchCode enforces safety through built-in rules and optional LLM validation.
The guard layer checks for:

- dangerous shell commands
- sensitive file access
- out-of-scope working directories
- undefined or risky tool usage

In interactive mode, uncertain actions can prompt for user confirmation.

## Refactoring Mode

Start with `--refactor` to enable SOLID refactoring behavior and to inject refactor playbook instructions into the system prompt.
This mode is useful for cleaner, rule-guided code improvements.

## Project Structure

- `src/agent.rs` — main agent loop and tool orchestration
- `src/guard/` — safety rules and guard agent
- `src/kg/` — knowledge graph indexing and KG tools
- `src/llm/` — language model provider bridge
- `src/refactor/` — refactoring workflows and playbook rules
- `src/tools/` — tool implementations
- `src/session.rs` — session persistence management
- `src/cost.rs` — token usage and cost tracking
- `src/compact.rs` — conversation compaction logic

## Development

Run tests with:

```bash
cargo test
```

Build in debug mode locally:

```bash
cargo run
```

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE) for details.

## Contributing

For contribution guidelines, see [CONTRIBUTING.md](CONTRIBUTING.md).
For community standards and behavior, see [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

