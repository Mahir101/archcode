/// Event types flowing from tools → agent loop → UI.
#[derive(Debug, Clone)]
pub enum PreviewType {
    Text,
    Guard,
    Tool,
    KG,
}

#[derive(Debug, Clone)]
pub struct Event {
    pub name: String,
    pub args: Vec<String>,
    pub message: String,
    pub preview_type: PreviewType,
    pub is_error: bool,
}

impl Event {
    pub fn tool(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            args: vec![],
            message: message.into(),
            preview_type: PreviewType::Tool,
            is_error: false,
        }
    }

    pub fn guard(tool_name: impl Into<String>, message: impl Into<String>, is_error: bool) -> Self {
        Self {
            name: "Guard".into(),
            args: vec![tool_name.into()],
            message: message.into(),
            preview_type: PreviewType::Guard,
            is_error,
        }
    }

    pub fn kg(message: impl Into<String>) -> Self {
        Self {
            name: "KG".into(),
            args: vec![],
            message: message.into(),
            preview_type: PreviewType::KG,
            is_error: false,
        }
    }
}
