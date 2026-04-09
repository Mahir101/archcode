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

/// A clonable handle that can stop the spinner from any thread/task.
#[derive(Clone)]
pub struct SpinnerHandle {
    stop: Arc<AtomicBool>,
}

impl SpinnerHandle {
    /// Signal the spinner to stop and clear its line.
    /// Non-blocking — the spinner thread will stop within ~80ms.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Animated terminal spinner with rotating fun messages.
pub struct Spinner {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Spinner {
    /// Start a spinner on a background thread. Returns `(Spinner, SpinnerHandle)`.
    /// The handle can be cloned and sent to other tasks to stop the spinner early.
    pub fn start() -> (Self, SpinnerHandle) {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();
        let ext_handle = SpinnerHandle { stop: stop.clone() };

        let handle = std::thread::spawn(move || {
            let mut frame_idx: usize = 0;
            let mut msg_idx: usize = 0;
            let mut ticks: usize = 0;

            while !stop_clone.load(Ordering::Relaxed) {
                let frame = FRAMES[frame_idx % FRAMES.len()];
                let msg = THINKING_MESSAGES[msg_idx % THINKING_MESSAGES.len()];

                eprint!("\r\x1b[36m{frame}\x1b[0m \x1b[90m{msg}\x1b[0m\x1b[K");
                let _ = std::io::stderr().flush();

                std::thread::sleep(FRAME_DELAY);
                frame_idx += 1;
                ticks += 1;

                if ticks.is_multiple_of(38) {
                    msg_idx += 1;
                }
            }

            // Clear the spinner line
            eprint!("\r\x1b[K");
            let _ = std::io::stderr().flush();
        });

        (
            Self {
                stop,
                handle: Some(handle),
            },
            ext_handle,
        )
    }

    /// Stop the spinner and wait for the thread to finish.
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
