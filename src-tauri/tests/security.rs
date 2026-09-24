//! What the commands refuse.
//!
//! Every case here is an argument a caller could send over the IPC channel.
//! The window cannot load remote content, so none of these is reachable by an
//! outside attacker, but the point of the exercise is that the backend holds
//! on its own rather than relying on the frontend to have shown a dialog.

use std::fs;
use std::path::PathBuf;

use allinsight_lib::services::ai::models;
use allinsight_lib::services::apps;
use allinsight_lib::services::startup;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("allinsight-sec-{}-{name}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    dir
}

#[test]
fn a_model_outside_the_managed_folder_is_not_removed() {
    let dir = scratch("model");
    let decoy = dir.join("not-ours.gguf");
    fs::write(&decoy, b"GGUF").unwrap();

    let error = models::remove(&decoy).unwrap_err();
    assert!(
        error.to_string().contains("models folder"),
        "expected a containment refusal, got: {error}"
    );
    assert!(decoy.exists(), "the file must still be there");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn traversal_out_of_the_models_folder_is_not_removed() {
    // A path that starts inside the managed folder and climbs out of it.
    let escape = models::model_directory()
        .join("..")
        .join("..")
        .join("target.gguf");
    let error = models::remove(&escape).unwrap_err();
    assert!(
        error.to_string().contains("models folder"),
        "expected a containment refusal, got: {error}"
    );
}

#[test]
fn only_a_gguf_is_removed_even_inside_the_managed_folder() {
    let inside = models::model_directory().join("settings.db");
    let error = models::remove(&inside).unwrap_err();
    assert!(
        error.to_string().contains("GGUF"),
        "expected an extension refusal, got: {error}"
    );
}

#[test]
fn an_application_reference_containing_a_separator_is_refused() {
    // Without this the reference would be read as a registry path and could
    // reach a key other than the one under Uninstall.
    for id in [
        r"Foo\..\..\Run",
        "Foo/Bar",
        r"..\..\..\Microsoft\Windows\CurrentVersion\Run",
        "",
    ] {
        let error = apps::uninstall(id).unwrap_err();
        assert!(
            error.to_string().contains("not valid")
                || error.to_string().contains("did not register")
                || error.to_string().contains("not available"),
            "id {id:?} was not refused, got: {error}"
        );
    }
}

#[test]
fn an_unknown_application_reference_starts_nothing() {
    let error = apps::uninstall("AllInsightDefinitelyNotInstalled_00000").unwrap_err();
    assert!(
        error.to_string().contains("did not register")
            || error.to_string().contains("not valid")
            || error.to_string().contains("not available"),
        "expected a lookup failure, got: {error}"
    );
}

#[test]
fn a_startup_reference_that_names_no_real_item_is_refused() {
    // The value name would otherwise be written into StartupApproved as given.
    for id in [
        "user-run:AllInsightNoSuchEntry_00000",
        r"user-run:..\..\Run",
        "machine-run:AllInsightNoSuchEntry_00000",
    ] {
        let error = startup::set_enabled(id, false).unwrap_err();
        let text = error.to_string();
        assert!(
            !text.contains("Could not update"),
            "id {id:?} reached the registry: {text}"
        );
    }
}

#[test]
fn a_startup_reference_with_no_location_tag_is_refused() {
    for id in ["", "no-colon-here", "not-a-location:Thing"] {
        assert!(
            startup::set_enabled(id, true).is_err(),
            "id {id:?} was accepted"
        );
    }
}

#[test]
fn the_engine_will_not_start_a_program_that_is_not_a_llama_server() {
    use allinsight_lib::services::ai::llama::{EngineConfig, LlamaEngine};

    // A real, present, executable file that is not the inference engine.
    let cmd = PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into()))
        .join("System32")
        .join("cmd.exe");
    if !cmd.is_file() {
        return;
    }

    let engine = LlamaEngine::new();
    let error = engine
        .start(EngineConfig {
            engine_path: cmd,
            model_path: models::model_directory().join("any.gguf"),
            context_size: 2048,
            gpu_layers: 0,
            threads: 2,
        })
        .unwrap_err();
    assert!(
        error.to_string().contains("llama-server.exe"),
        "expected the engine name to be checked, got: {error}"
    );
}

/// The cleanup claim exists to stop two passes running over the same folders.
/// It has to be asymmetric: a pass the user asked for wins, and the hourly
/// background pass gives way rather than making the button refuse.
#[test]
fn a_cleanup_the_user_asked_for_takes_precedence_over_the_automatic_one() {
    use allinsight_lib::state::{AppState, CleanupOwner};

    let Ok(state) = AppState::new() else {
        // No writable data folder here; nothing to assert against.
        return;
    };
    // The lease borrows the state, and one of them is handed to another
    // thread below. Leaking it in a test that runs once is simpler than
    // reference counting for the sake of the borrow checker.
    let state: &'static AppState = Box::leak(Box::new(state));

    // Nothing running: either caller may start.
    {
        let lease = state.begin_cleanup(CleanupOwner::User).unwrap();
        // A second user-initiated pass is a double click, and is refused.
        assert!(state.begin_cleanup(CleanupOwner::User).is_err());
        // So is the automatic one, which simply waits for the next hour.
        assert!(state.begin_cleanup(CleanupOwner::Automatic).is_err());
        drop(lease);
    }

    // The claim is released with the lease, so the next pass can start.
    let automatic = state.begin_cleanup(CleanupOwner::Automatic).unwrap();

    // The user now asks for one. The automatic pass is told to stop, and this
    // call waits for it; here nothing is actually running, so release it from
    // another thread the way a real pass would when it sees the flag.
    let handle = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(150));
        drop(automatic);
    });

    let user = state.begin_cleanup(CleanupOwner::User);
    handle.join().unwrap();
    assert!(
        user.is_ok(),
        "the user's cleanup did not preempt the automatic one: {:?}",
        user.err()
    );
}
