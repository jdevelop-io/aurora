//! Terminal output formatting with rich UI support.

#![allow(dead_code)]

use std::time::Duration;

use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

/// Prints a success message.
pub fn success(message: &str) {
    println!("{} {}", style("✓").green().bold(), message);
}

/// Prints an error message.
pub fn error(message: &str) {
    eprintln!("{} {}", style("✗").red().bold(), message);
}

/// Prints a warning message.
pub fn warning(message: &str) {
    println!("{} {}", style("⚠").yellow().bold(), message);
}

/// Prints an info message.
pub fn info(message: &str) {
    println!("{} {}", style("ℹ").blue().bold(), message);
}

/// Prints a beam execution header.
pub fn beam_header(name: &str) {
    println!(
        "\n{} {}",
        style("▶").cyan().bold(),
        style(name).cyan().bold()
    );
}

/// Prints a beam skipped message.
pub fn beam_skipped(name: &str) {
    println!(
        "{} {} {}",
        style("○").dim(),
        style(name).dim(),
        style("(cached)").dim()
    );
}

/// Prints a beam completed message.
pub fn beam_completed(name: &str, duration_ms: u64) {
    println!(
        "{} {} {}",
        style("✓").green(),
        name,
        style(format!("({}ms)", duration_ms)).dim()
    );
}

/// Prints a beam failed message.
pub fn beam_failed(name: &str, error: &str) {
    eprintln!("{} {} - {}", style("✗").red(), style(name).red(), error);
}

/// Prints a summary of the execution.
pub fn summary(executed: usize, skipped: usize, failed: usize, duration_ms: u64) {
    println!();

    if failed > 0 {
        println!(
            "{}: {} executed, {} skipped, {} failed in {}ms",
            style("FAILED").red().bold(),
            executed,
            skipped,
            failed,
            duration_ms
        );
    } else {
        println!(
            "{}: {} executed, {} skipped in {}ms",
            style("SUCCESS").green().bold(),
            executed,
            skipped,
            duration_ms
        );
    }
}

// ============================================================================
// Rich UI Components
// ============================================================================

/// Creates a spinner for long-running operations.
pub fn create_spinner(message: &str) -> ProgressBar {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .expect("Invalid spinner template"),
    );
    spinner.set_message(message.to_string());
    spinner.enable_steady_tick(Duration::from_millis(80));
    spinner
}

/// Creates a progress bar with a specific length.
pub fn create_progress_bar(len: u64, message: &str) -> ProgressBar {
    let pb = ProgressBar::new(len);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .expect("Invalid progress bar template")
            .progress_chars("█▓░"),
    );
    pb.set_message(message.to_string());
    pb
}

/// Creates a multi-progress container for parallel operations.
pub fn create_multi_progress() -> MultiProgress {
    MultiProgress::new()
}

/// Creates a beam execution spinner.
pub fn beam_spinner(name: &str) -> ProgressBar {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .expect("Invalid spinner template"),
    );
    spinner.set_message(format!("{} {}", style("▶").cyan(), name));
    spinner.enable_steady_tick(Duration::from_millis(80));
    spinner
}

/// Finishes a spinner with success.
pub fn spinner_success(spinner: &ProgressBar, message: &str) {
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{msg}")
            .expect("Invalid spinner template"),
    );
    spinner.finish_with_message(format!("{} {}", style("✓").green(), message));
}

/// Finishes a spinner with failure.
pub fn spinner_failure(spinner: &ProgressBar, message: &str) {
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{msg}")
            .expect("Invalid spinner template"),
    );
    spinner.finish_with_message(format!("{} {}", style("✗").red(), message));
}

/// Finishes a spinner with skip status.
pub fn spinner_skipped(spinner: &ProgressBar, message: &str) {
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{msg}")
            .expect("Invalid spinner template"),
    );
    spinner.finish_with_message(format!(
        "{} {} {}",
        style("○").dim(),
        style(message).dim(),
        style("(cached)").dim()
    ));
}

/// Prints a header for a section.
pub fn section_header(title: &str) {
    println!("\n{}", style(format!("── {} ──", title)).bold());
}

/// Prints a list item.
pub fn list_item(text: &str) {
    println!("  {} {}", style("•").dim(), text);
}

/// Prints a key-value pair.
pub fn key_value(key: &str, value: &str) {
    println!("  {}: {}", style(key).dim(), value);
}

/// Prints a divider line.
pub fn divider() {
    println!("{}", style("─".repeat(60)).dim());
}
