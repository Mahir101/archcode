use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::llm::Message;

const SESSION_DIR: &str = ".archcode/sessions";

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
    pub model: String,
    pub message_count: usize,
    pub summary: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SessionData {
    pub meta: SessionMeta,
    pub messages: Vec<Message>,
}

pub struct SessionManager {
    base_dir: PathBuf,
}

impl SessionManager {
    pub fn new(cwd: &str) -> Self {
        Self {
            base_dir: Path::new(cwd).join(SESSION_DIR),
        }
    }

    fn ensure_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.base_dir)?;
        Ok(())
    }

    fn session_path(&self, id: &str) -> PathBuf {
        self.base_dir.join(format!("{id}.json"))
    }

    pub fn save(&self, id: &str, model: &str, messages: &[Message], summary: &str) -> Result<()> {
        self.ensure_dir()?;
        let path = self.session_path(id);

        let now = chrono_now();
        let meta = SessionMeta {
            id: id.to_string(),
            created_at: if path.exists() {
                // Preserve original creation time
                if let Ok(existing) = self.load_data(id) {
                    existing.meta.created_at
                } else {
                    now.clone()
                }
            } else {
                now.clone()
            },
            updated_at: now,
            model: model.to_string(),
            message_count: messages.len(),
            summary: summary.to_string(),
        };

        let data = SessionData {
            meta,
            messages: messages.to_vec(),
        };

        let json = serde_json::to_string_pretty(&data)?;
        fs::write(&path, json)?;
        Ok(())
    }

    pub fn load(&self, id: &str) -> Result<(SessionMeta, Vec<Message>)> {
        let data = self.load_data(id)?;
        Ok((data.meta, data.messages))
    }

    fn load_data(&self, id: &str) -> Result<SessionData> {
        let path = self.session_path(id);
        let content = fs::read_to_string(&path)?;
        let data: SessionData = serde_json::from_str(&content)?;
        Ok(data)
    }

    pub fn list(&self) -> Vec<SessionMeta> {
        let mut sessions = vec![];
        if let Ok(entries) = fs::read_dir(&self.base_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "json").unwrap_or(false) {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(data) = serde_json::from_str::<SessionData>(&content) {
                            sessions.push(data.meta);
                        }
                    }
                }
            }
        }
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        sessions
    }

    #[allow(dead_code)]
    pub fn delete(&self, id: &str) -> Result<()> {
        let path = self.session_path(id);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }
}

fn chrono_now() -> String {
    // Simple ISO-8601 timestamp without chrono crate
    use std::time::SystemTime;
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Format as Unix timestamp (sufficient for ordering)
    format!("{secs}")
}

/// Generate a short summary from the first user message.
pub fn auto_summary(messages: &[Message]) -> String {
    for msg in messages {
        if msg.role == crate::llm::Role::User {
            let text = msg.text();
            if !text.is_empty() {
                let truncated: String = text.chars().take(80).collect();
                if text.len() > 80 {
                    return format!("{truncated}...");
                }
                return truncated;
            }
        }
    }
    "Empty session".to_string()
}
