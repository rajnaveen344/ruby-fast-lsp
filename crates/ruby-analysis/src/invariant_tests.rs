use crate::invariant::ExpectInvariant;
use std::cell::Cell;
use std::fmt;
use std::panic::{catch_unwind, AssertUnwindSafe};

fn panic_message(run: impl FnOnce()) -> String {
    let payload = catch_unwind(AssertUnwindSafe(run))
        .expect_err("the invariant check under test should have panicked");
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    payload
        .downcast_ref::<&str>()
        .map(|message| (*message).to_string())
        .expect_invariant(
            "panic payload is neither String nor &str",
            "invariant macros panic with formatted text",
            "panic through the invariant macros only",
        )
}

/// Counts how many times it was formatted.
struct Counted<'a>(&'a Cell<usize>);

impl fmt::Display for Counted<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.set(self.0.get() + 1);
        formatter.write_str("counted")
    }
}

#[test]
fn invariant_renders_what_why_and_fix_with_arguments_in_any_part() {
    let id = 7;
    let message = panic_message(|| {
        invariant!(
            id == 0,
            what = "file {id} is unknown",
            why = "ids {} are registered first",
            fix = "register {name}",
            "before use",
            id = id,
            name = "it",
        )
    });
    assert_eq!(
        message,
        "invariant violated: file 7 is unknown — bug: ids before use are registered first — fix: register it"
    );
}

#[test]
fn invariant_without_arguments_keeps_brace_escapes() {
    let message = panic_message(|| {
        invariant!(
            false,
            what = "set {{}} is empty",
            why = "callers fill it",
            fix = "fill it",
        )
    });
    assert_eq!(
        message,
        "invariant violated: set {} is empty — bug: callers fill it — fix: fill it"
    );
}

#[test]
fn invariant_formats_its_message_only_when_it_fails() {
    let calls = Cell::new(0);
    invariant!(true, what = "{}", why = "w", fix = "f", Counted(&calls));
    invariant_eq!(1, 1, what = "{}", why = "w", fix = "f", Counted(&calls));
    invariant_ne!(1, 2, what = "{}", why = "w", fix = "f", Counted(&calls));
    assert_eq!(calls.get(), 0);

    let message =
        panic_message(|| invariant!(false, what = "{}", why = "w", fix = "f", Counted(&calls)));
    assert_eq!(message, "invariant violated: counted — bug: w — fix: f");
    assert_eq!(calls.get(), 1);
}

#[test]
fn invariant_eq_and_ne_print_both_operands() {
    let message = panic_message(|| invariant_eq!(1, 2, what = "a", why = "b", fix = "c"));
    assert!(
        message.contains("left: 1") && message.contains("right: 2"),
        "{message}"
    );
    assert!(
        message.contains("invariant violated: a — bug: b — fix: c"),
        "{message}"
    );

    let message = panic_message(|| invariant_ne!(3, 3, what = "x {}", why = "y", fix = "z", 9));
    assert!(message.contains("left: 3"), "{message}");
    assert!(
        message.contains("invariant violated: x 9 — bug: y — fix: z"),
        "{message}"
    );
}

#[test]
fn unreachable_invariant_renders_the_shared_message() {
    let state = "closed";
    let message = panic_message(|| {
        unreachable_invariant!(
            what = "state {state} reached dispatch",
            why = "closed states are filtered",
            fix = "filter first",
            state = state,
        )
    });
    assert_eq!(
        message,
        "invariant violated: state closed reached dispatch — bug: closed states are filtered — fix: filter first"
    );
}

#[test]
fn expect_invariant_returns_present_values() {
    assert_eq!(Some(3).expect_invariant("a", "b", "c"), 3);
    assert_eq!(Ok::<_, String>(4).expect_invariant("a", "b", "c"), 4);
}

#[test]
fn expect_invariant_on_none_renders_the_shared_message() {
    let message = panic_message(|| {
        None::<u8>.expect_invariant("slot is empty", "slots are filled at startup", "fill it");
    });
    assert_eq!(
        message,
        "invariant violated: slot is empty — bug: slots are filled at startup — fix: fill it"
    );
}

#[test]
fn expect_invariant_on_err_appends_the_error_to_what() {
    let message = panic_message(|| {
        Err::<u8, _>("disk gone").expect_invariant("write failed", "the file is owned", "retry");
    });
    assert_eq!(
        message,
        "invariant violated: write failed: \"disk gone\" — bug: the file is owned — fix: retry"
    );
}
