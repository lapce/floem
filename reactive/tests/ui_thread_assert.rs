use floem_reactive::Runtime;

fn panic_message(err: Box<dyn std::any::Any + Send + 'static>) -> String {
    match err.downcast::<String>() {
        Ok(msg) => *msg,
        Err(err) => match err.downcast::<&'static str>() {
            Ok(msg) => (*msg).to_string(),
            Err(other) => format!("{other:?}"),
        },
    }
}

#[test]
fn assert_ui_thread_reports_caller_and_set_location() {
    // Establish the UI thread (main thread).
    Runtime::init_on_ui_thread();

    let result = std::thread::spawn(|| std::panic::catch_unwind(|| Runtime::assert_ui_thread()))
        .join()
        .expect("thread join");

    #[cfg(debug_assertions)]
    {
        let message = panic_message(result.expect_err("expected panic off UI thread"));
        assert!(
            message.contains("Unsync runtime access from non-UI thread"),
            "panic message should describe unsync access; got {message}"
        );
        assert!(
            message.contains("caller:"),
            "panic message should include caller location; got {message}"
        );
        assert!(
            message.contains("set at"),
            "panic message should include UI init location; got {message}"
        );
        assert!(
            message.contains(file!()),
            "panic message should reference this file as caller; got {message}"
        );
        assert!(
            !message.contains("reactive/src/runtime.rs"),
            "panic message should point to user/test code, not runtime internals; got {message}"
        );
    }

    #[cfg(not(debug_assertions))]
    {
        assert!(
            result.is_err(),
            "assert_ui_thread should panic off UI thread"
        );
    }
}
