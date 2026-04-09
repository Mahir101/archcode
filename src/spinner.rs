use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const FRAME_DELAY: Duration = Duration::from_millis(80);

const THINKING_MESSAGES: &[&str] = &[
    "Thinking...",
    "Analyzing the codebase...",
    "Reasoning about approach...",
    "Mapping code structure...",
    "Building mental model...",
    "Evaluating options...",
    "Crafting a response...",
    "Connecting the dots...",
    "Reviewing context...",
    "Formulating plan...",
    "Processing your request...",
    "Digging through the code...",
    "Synthesizing information...",
    "Exploring possibilities...",
    "Weighing trade-offs...",
    "Almost there...",
    "Considering edge cases...",
    "Putting it all together...",
    "Working through the logic...",
    "Generating solution...",
];

/// Animated terminal spinner with rotating fun messages.
pub struct Spinner {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Spinner {
    /// Start a spinner on a background thread. It writes to stderr so it
    /// doesn't interfere with stdout content.
    pub fn start() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();

        let handle = std::thread::spawn(move || {
            let mut frame_idx: usize = 0;
            let mut msg_idx: usize = 0;
            let mut ticks: usize = 0;

            while !stop_clone.load(Ordering::Relaxed) {
                let frame = FRAMES[frame_idx % FRAMES.len()];
                let msg = THINKING_MESSAGES[msg_idx % THINKING_MESSAGES.len()];

                // \r moves to line start, \x1b[K clears the rest of line
                eprint!("\r\x1b[36m{frame}\x1b[0m \x1b[90m{msg}\x1b[0m\x1b[K");
                let _ = std::io::stderr().flush();

                std::thread::sleep(FRAME_DELAY);
                frame_idx += 1;
                ticks += 1;

                // Rotate message every ~3 seconds
                if ticks.is_multiple_of(38) {
                    msg_idx += 1;
                }
            }

            // Clear the spinner line
            eprint!("\r\x1b[K");
            let _ = std::io::stderr().flush();
        });

        Self {
            stop,
            handle: Some(handle),
        }
    }

    /// Stop the spinner and clear the line.
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
