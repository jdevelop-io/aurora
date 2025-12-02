//! Terminal output formatting.

#![allow(dead_code)]

use console::style;

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
