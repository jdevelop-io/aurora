//! A panic inside the TUI must restore the terminal instead of leaving the
//! user's shell in raw mode. The restore is wired through a panic hook that
//! still delegates to the previously installed hook, so panic messages are
//! not swallowed.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[test]
fn panic_hook_delegates_to_the_previous_hook() {
    let previous_ran = Arc::new(AtomicBool::new(false));
    let flag = previous_ran.clone();

    std::panic::set_hook(Box::new(move |_| {
        flag.store(true, Ordering::SeqCst);
    }));

    // Chains the terminal-restoring hook on top of the one set above.
    aurora_tui::install_terminal_panic_hook();

    let result = std::panic::catch_unwind(|| panic!("boom"));

    assert!(result.is_err(), "the panic must still propagate");
    assert!(
        previous_ran.load(Ordering::SeqCst),
        "the previously installed hook must still run (panic message preserved)"
    );

    let _ = std::panic::take_hook();
}
