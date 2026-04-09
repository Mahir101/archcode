use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Tracks token usage and estimated cost across a session.
#[derive(Clone)]
pub struct CostTracker {
    inner: Arc<Mutex<CostInner>>,
}

struct CostInner {
    total_input_tokens: u64,
    total_output_tokens: u64,
    api_calls: u64,
    model: String,
    started_at: Instant,
}

impl CostTracker {
    pub fn new(model: &str) -> Self {
        Self {
            inner: Arc::new(Mutex::new(CostInner {
                total_input_tokens: 0,
                total_output_tokens: 0,
                api_calls: 0,
                model: model.to_string(),
                started_at: Instant::now(),
            })),
        }
    }

    pub fn record(&self, input_tokens: u64, output_tokens: u64) {
        let mut inner = self.inner.lock().unwrap();
        inner.total_input_tokens += input_tokens;
        inner.total_output_tokens += output_tokens;
        inner.api_calls += 1;
    }

    pub fn summary(&self) -> CostSummary {
        let inner = self.inner.lock().unwrap();
        let total_tokens = inner.total_input_tokens + inner.total_output_tokens;
        let cost_usd = estimate_cost(&inner.model, inner.total_input_tokens, inner.total_output_tokens);
        CostSummary {
            input_tokens: inner.total_input_tokens,
            output_tokens: inner.total_output_tokens,
            total_tokens,
            api_calls: inner.api_calls,
            estimated_cost_usd: cost_usd,
            elapsed_secs: inner.started_at.elapsed().as_secs(),
            model: inner.model.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CostSummary {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub api_calls: u64,
    pub estimated_cost_usd: f64,
    pub elapsed_secs: u64,
    pub model: String,
}

impl std::fmt::Display for CostSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let elapsed = format_duration(self.elapsed_secs);
        write!(
            f,
            "Model: {}\n\
             API calls: {}\n\
             Input tokens:  {:>10}\n\
             Output tokens: {:>10}\n\
             Total tokens:  {:>10}\n\
             Est. cost: ${:.4}\n\
             Session time: {}",
            self.model,
            self.api_calls,
            self.input_tokens,
            self.output_tokens,
            self.total_tokens,
            self.estimated_cost_usd,
            elapsed,
        )
    }
}

/// Rough cost estimation based on model name.
fn estimate_cost(model: &str, input: u64, output: u64) -> f64 {
    let (input_per_m, output_per_m) = match model {
        m if m.starts_with("gpt-4o-mini") => (0.15, 0.60),
        m if m.starts_with("gpt-4o") => (2.50, 10.00),
        m if m.starts_with("gpt-4-turbo") => (10.00, 30.00),
        m if m.starts_with("gpt-4") => (30.00, 60.00),
        m if m.starts_with("gpt-3.5") => (0.50, 1.50),
        m if m.starts_with("claude-3-5-sonnet") || m.starts_with("claude-sonnet") => (3.00, 15.00),
        m if m.starts_with("claude-3-5-haiku") || m.starts_with("claude-haiku") => (0.25, 1.25),
        m if m.starts_with("claude-3-opus") || m.starts_with("claude-opus") => (15.00, 75.00),
        m if m.starts_with("o1-mini") => (3.00, 12.00),
        m if m.starts_with("o1") => (15.00, 60.00),
        m if m.starts_with("o3-mini") => (1.10, 4.40),
        _ => (0.0, 0.0), // Local models (Ollama) or unknown — free
    };
    (input as f64 / 1_000_000.0 * input_per_m) + (output as f64 / 1_000_000.0 * output_per_m)
}

fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}
