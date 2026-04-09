//! Semantic color constants for consistent terminal output.
//! All values are ANSI escape sequences.
#![allow(dead_code)]

// --- Primary colors ---
pub const PRIMARY: &str = "\x1b[1;36m";      // Bold cyan
pub const SECONDARY: &str = "\x1b[0;36m";    // Cyan
pub const ACCENT: &str = "\x1b[1;35m";       // Bold magenta

// --- Status colors ---
pub const SUCCESS: &str = "\x1b[1;32m";      // Bold green
pub const WARNING: &str = "\x1b[0;33m";      // Yellow
pub const ERROR: &str = "\x1b[1;31m";        // Bold red
pub const INFO: &str = "\x1b[0;34m";         // Blue

// --- UI elements ---
pub const MUTED: &str = "\x1b[0;90m";        // Dark gray
pub const BOLD: &str = "\x1b[1;37m";         // Bold white
pub const DIM: &str = "\x1b[2m";             // Dim
pub const RESET: &str = "\x1b[0m";           // Reset all

// --- Specific UI roles ---
pub const PROMPT: &str = "\x1b[1;32m";       // Bold green (user input prompt)
pub const ASSISTANT: &str = "\x1b[0;37m";    // White (assistant responses)
pub const TOOL_LABEL: &str = "\x1b[0;36m";   // Cyan (tool events)
pub const GUARD_LABEL: &str = "\x1b[0;33m";  // Yellow (guard events)
pub const KG_LABEL: &str = "\x1b[0;35m";     // Magenta (KG events)
pub const SESSION_LABEL: &str = "\x1b[0;36m"; // Cyan (session events)
pub const COST_LABEL: &str = "\x1b[0;90m";   // Gray (cost info)
pub const STREAM_TEXT: &str = "\x1b[0;37m";  // White (streaming text)
