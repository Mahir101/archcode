# Rapcode

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](http://makeapullrequest.com)

**Rapcode** is an agentic AI coding assistant created by Mahir101, designed to help developers build, explore, and refactor codebases safely and intelligently. Written in Rust, it comes with multi-agent orchestration, an extensible toolset, a built-in safety guard, and deep understanding of code structures through a local knowledge graph.

## Features

- **Agentic AI Assistant**: Solves complex coding tasks, understands your project structure, and makes targeted changes.
- **Multi-Agent Orchestration**: Handles multiple specialized agents (like a guard agent for safety and a main agent for task execution).
- **Safety Guards**: Includes built-in safety validations (DangerousCommandRule, SensitiveFileRule, WorkingDirRule) and an LLM-based guard agent to prevent unintended destructive actions.
- **Knowledge Graph (KG) Integration**: Builds a local graph of your codebase to understand relationships, assess impact, trace dependencies, and evaluate risks.
- **SOLID Refactoring Mode**: Dedicated refactoring capabilities built in, including baseline tests, formatting, linting, semantic pattern matching (semgrep), and git diff integrations.
- **Comprehensive Toolset**:
  - **Filesystem**: Read, Write, Edit, Glob
  - **Execution**: Execute bash commands
  - **Workflow**: Manage todos (TodoRead, TodoWrite)
  - **Information Retrieval**: Perform Web Searches and KG queries

## Installation

Ensure you have Rust and Cargo installed, then build the project:

```bash
cargo build --release
```

The compiled binary will be located at `target/release/archcode`.

## Usage

Rapcode is executed from the terminal. You can run it via `cargo run` during development, or execute the compiled binary from the `target/` directory.

### Interactive Mode

Simply run the binary without arguments to enter the interactive chat interface:

```bash
cargo run
# or if built in release mode:
./target/release/archcode
```

### Single-shot Prompt

If you want to run a specific command non-interactively, use the `--prompt` flag:

```bash
cargo run -- --prompt "Refactor the src/utils.rs file to reduce duplication"
# or
./target/release/archcode --prompt "Refactor the src/utils.rs file to reduce duplication"
```

### Flags & Options

- `--prompt <STRING>`: Single-shot prompt (non-interactive).
- `--no-guard`: Disables the safety guard agent (use with caution).
- `--refactor`: Enables SOLID Refactoring Mode. Injects playbook rules into the system prompt and prioritizes refactoring tools.

```bash
./target/release/archcode --refactor --prompt "Analyze my codebase for SOLID principle smells"
```

## Project Structure

- `src/agent.rs`: Core multi-agent logic and orchestration.
- `src/guard/`: Safety components and rule validations.
- `src/kg/`: Knowledge Graph implementation for mapping codebase relationships.
- `src/llm/`: Providers for language models (Anthropic, OpenAI, etc.).
- `src/refactor/`: Refactoring specific workflows and tool integrations.
- `src/skills/` & `src/tools/`: The extensive capabilities and tools the agent can use.
- `refactoring/`: Playbooks, rules, and documentation for the SOLID refactoring module.

## Configuration

Rapcode relies on standard environment variables to connect to LLM providers:
- `OPENAI_API_KEY` (if using OpenAI)
- `ANTHROPIC_API_KEY` (if using Anthropic)

You can define these in a `.env` file at the root of your project or export them in your shell session.

## Contributing

We welcome contributions to Rapcode! Please see our [Contributing Guidelines](CONTRIBUTING.md) for more details on how to get started, set up your development environment, and submit Pull Requests.

Please note that this project is released with a [Contributor Code of Conduct](CODE_OF_CONDUCT.md). By participating in this project you agree to abide by its terms.

## License & Authors

This project is licensed under the [MIT License](LICENSE) - see the LICENSE file for details.

Created by **Mahir101**. 
