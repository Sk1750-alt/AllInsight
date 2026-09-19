//! Running WMI queries on a thread that owns the right COM apartment.
//!
//! Tauri executes a synchronous command on the main thread, and the main
//! thread of a Windows GUI application is a single-threaded COM apartment.
//! The `wmi` crate initialises COM as multi-threaded, so calling it from there
//! either fails outright with `RPC_E_CHANGED_MODE` or returns nothing at all -
//! which is worse, because an empty result set is indistinguishable from a
//! machine that genuinely has no drives.
//!
//! Every WMI query therefore runs on a freshly spawned thread, which starts
//! with no apartment and is free to join the multi-threaded one. Spawning a
//! thread costs tens of microseconds against a query that costs milliseconds,
//! and it keeps the query off the UI thread as a side benefit.

/// Run `f` on a thread with a clean COM state and wait for the result.
///
/// A panic inside `f` is converted into an error rather than being allowed to
/// unwind into the caller: a broken WMI provider must not take down a command.
pub fn run<T, F>(what: &'static str, f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let handle = std::thread::Builder::new()
        .name(format!("allinsight-wmi-{what}"))
        .spawn(f)
        .map_err(|e| format!("Could not start the {what} query: {e}"))?;

    handle.join().map_err(|_| {
        format!("The {what} query stopped unexpectedly and was ignored.")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_comes_back_from_the_worker() {
        assert_eq!(run("test", || 6 * 7).unwrap(), 42);
    }

    #[test]
    fn a_panic_becomes_an_error_rather_than_unwinding() {
        let result = run("test", || -> u32 { panic!("provider exploded") });
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("stopped unexpectedly"));
    }
}
