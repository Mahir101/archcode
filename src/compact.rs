use crate::llm::{Message, Role};

/// Rough token count estimate: ~4 chars per token on average.
pub fn estimate_tokens(messages: &[Message]) -> usize {
    messages
        .iter()
        .map(|m| {
            let text_len = m.text().len();
            let tool_calls_len: usize = m
                .tool_calls()
                .iter()
                .map(|tc| tc.name.len() + tc.arguments.len())
                .sum();
            (text_len + tool_calls_len) / 4 + 1
        })
        .sum()
}

/// Check if context should be compacted.
/// Returns true when estimated tokens exceed the threshold.
pub fn should_compact(messages: &[Message], max_context_tokens: usize) -> bool {
    let threshold = (max_context_tokens as f64 * 0.85) as usize;
    estimate_tokens(messages) > threshold
}

/// Compact messages by summarizing old conversation, keeping:
/// - System prompt (first message)
/// - Last `keep_recent` messages
/// - A summary message of everything in between
pub fn compact(messages: &[Message], keep_recent: usize) -> Vec<Message> {
    if messages.len() <= keep_recent + 2 {
        return messages.to_vec(); // Nothing to compact
    }

    let system_msg = &messages[0]; // System prompt
    let cutoff = messages.len().saturating_sub(keep_recent);
    let old_messages = &messages[1..cutoff];
    let recent_messages = &messages[cutoff..];

    // Build summary of old messages
    let mut summary_parts = vec![];
    for msg in old_messages {
        match msg.role {
            Role::User => {
                let text = msg.text();
                if !text.is_empty() {
                    let short: String = text.chars().take(200).collect();
                    summary_parts.push(format!("User asked: {short}"));
                }
            }
            Role::Assistant => {
                let text = msg.text();
                if !text.is_empty() {
                    let short: String = text.chars().take(200).collect();
                    summary_parts.push(format!("Assistant: {short}"));
                }
                let tcs = msg.tool_calls();
                if !tcs.is_empty() {
                    let names: Vec<&str> = tcs.iter().map(|tc| tc.name.as_str()).collect();
                    summary_parts.push(format!("Used tools: {}", names.join(", ")));
                }
            }
            Role::Tool => {
                // Skip tool results in summary to save tokens
            }
            Role::System => {}
        }
    }

    let summary_text = format!(
        "[Context compacted — {} earlier messages summarized]\n\n{}",
        old_messages.len(),
        summary_parts.join("\n")
    );

    let mut compacted = vec![system_msg.clone()];
    compacted.push(Message::user(summary_text));
    compacted.push(Message::assistant(
        "Understood. I have the conversation context. How can I help?",
    ));
    compacted.extend(recent_messages.iter().cloned());

    compacted
}
