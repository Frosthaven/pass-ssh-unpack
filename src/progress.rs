use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

/// Create a spinner for indeterminate operations
pub fn spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

/// Create a progress bar for vault processing
pub fn vault_progress_bar(len: u64) -> ProgressBar {
    let pb = ProgressBar::new(len);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("Processing vaults [{bar:20.cyan/dim}] {pos}/{len}")
            .unwrap()
            .progress_chars("━━─"),
    );
    pb
}

/// Create a progress bar for rclone operations with current item display
pub fn rclone_progress_bar(len: u64) -> ProgressBar {
    let pb = ProgressBar::new(len);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("  [{bar:20.cyan/dim}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("━━─"),
    );
    pb
}

/// Create a progress bar for Teleport node processing
pub fn node_progress_bar(len: u64) -> ProgressBar {
    let pb = ProgressBar::new(len);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("Processing Nodes [{bar:20.cyan/dim}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("━━─"),
    );
    pb
}
