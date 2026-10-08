//! Local AI commands.
//!
//! Note what is absent: there is no command that takes text from the model and
//! does anything with it. The model produces a string, the string is displayed,
//! and that is the end of it.

use std::path::PathBuf;

use tauri::State;

use crate::error::{AllInsightError, Result};
use crate::services::ai::{self, models, AiAnswer, EngineConfig, EngineStatus, ModelInventory};
use crate::state::AppState;

#[tauri::command]
pub async fn get_ai_status(state: State<'_, AppState>) -> Result<EngineStatus> {
    Ok(state.engine.status())
}

#[tauri::command]
pub async fn get_local_models(state: State<'_, AppState>) -> Result<ModelInventory> {
    Ok({
        let settings = state.settings();
        models::inventory(settings.ai_engine_path.as_deref())
    })
}

/// Copy a GGUF the user chose into the managed folder.
///
/// The file is validated by reading its header, not by trusting the name.
#[tauri::command]
pub async fn import_local_model(path: String) -> Result<models::LocalModel> {
    models::import(std::path::Path::new(&path))
}

#[tauri::command]
pub async fn remove_local_model(path: String) -> Result<()> {
    models::remove(std::path::Path::new(&path))
}

/// Start the local engine with the configured model.
///
/// Blocking, and it can take a minute on a large model, so the interface shows
/// a loading state rather than assuming it is instant.
#[tauri::command]
pub async fn load_ai_model(
    state: State<'_, AppState>,
    model_path: Option<String>,
) -> Result<EngineStatus> {
    let settings = state.settings();
    if !settings.ai_enabled {
        return Err(AllInsightError::Ai(
            "The local assistant is turned off in Settings.".into(),
        ));
    }

    let model = model_path
        .map(PathBuf::from)
        .or(settings.ai_model_path.clone())
        .ok_or_else(|| {
            AllInsightError::Ai("No model is selected. Import a GGUF file in Settings.".into())
        })?;

    let engine_path = models::find_engine(settings.ai_engine_path.as_deref()).ok_or_else(|| {
        AllInsightError::Ai(
            "The inference engine was not found. Put llama-server.exe in the AllInsight engine folder, or choose it in Settings."
                .into(),
        )
    })?;

    let status = state.engine.start(EngineConfig {
        engine_path,
        model_path: model.clone(),
        context_size: settings.ai_context_size,
        threads: settings.ai_threads,
        gpu_layers: settings.ai_gpu_layers,
    })?;

    // Remember the model that actually loaded, so the next launch is one click.
    let mut next = settings;
    next.ai_model_path = Some(model);
    let _ = state.update_settings(next);

    let _ = state.db.log_activity("ai", "Loaded the local model", None);
    Ok(status)
}

#[tauri::command]
pub async fn unload_ai_model(state: State<'_, AppState>) -> Result<EngineStatus> {
    Ok({
        state.engine.stop();
        state.engine.status()
    })
}

#[tauri::command]
pub async fn get_ai_insight(state: State<'_, AppState>) -> Result<AiAnswer> {
    Ok({
        let facts = state.facts();
        ai::overview_insight(&state.engine, &facts)
    })
}

#[tauri::command]
pub async fn ask_ai(state: State<'_, AppState>, question: String) -> Result<AiAnswer> {
    let facts = state.facts();
    let answer = ai::answer(&state.engine, &facts, &question)?;

    // Unless the user asked for the model to stay resident, release it once it
    // has answered. Several gigabytes should not stay committed for a feature
    // used occasionally, and reloading takes seconds.
    if answer.from_model && !state.settings().ai_keep_loaded {
        state.engine.stop();
    }

    // Only that a question was asked. Never the question itself, and never the
    // answer: this log exists for troubleshooting, not for a transcript.
    let _ = state.db.log_activity(
        "ai",
        if answer.from_model {
            "Answered a question using the local model"
        } else {
            "Answered a question from measurements"
        },
        None,
    );
    Ok(answer)
}

/// The facts the model would be given, so the user can see exactly what leaves
/// the deterministic layer. Nothing is hidden from them.
#[tauri::command]
pub async fn get_ai_context(state: State<'_, AppState>) -> Result<String> {
    Ok(state.facts().briefing())
}

#[tauri::command]
pub async fn get_insights(state: State<'_, AppState>) -> Result<Vec<ai::Insight>> {
    Ok(ai::insights::generate(&state.facts()))
}

#[tauri::command]
pub async fn get_device_score(state: State<'_, AppState>) -> Result<ai::DeviceScore> {
    Ok(ai::device_score(&state.facts()))
}
