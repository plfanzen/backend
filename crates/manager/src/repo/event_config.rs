// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Mutex;

use boa_engine::value::TryIntoJs;
use boa_engine::{JsError, JsNativeError, JsValue};
use serde::{Deserialize, Serialize};

use crate::{js::create_boa_context, repo::challenges::metadata::CtfChallengeMetadata};
use boa_engine::{NativeFunction, Source, js_string, js_value, object::builtins::JsFunction};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CtfCategory {
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CtfDifficulty {
    pub name: String,
    pub color: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EventConfig {
    pub event_name: String,
    pub front_page_md: String,
    pub rules_md: String,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub end_time: chrono::DateTime<chrono::Utc>,
    pub use_teams: bool,
    pub registration_start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub registration_end_time: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_team_size: Option<u32>,
    pub scoreboard_freeze_time: Option<chrono::DateTime<chrono::Utc>>,
    // JS code that calls setPointsFn((challengeMetadata, currentSolves, solveIndex) => points);
    #[serde(skip_serializing_if = "Option::is_none")]
    pub points_fn: Option<String>,
    pub categories: HashMap<String, CtfCategory>,
    pub difficulties: HashMap<String, CtfDifficulty>,
}

impl EventConfig {
    pub async fn try_load_from_repo(
        repo_dir: &std::path::Path,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let event_config_path = repo_dir.join("event.yml");
        let file = std::fs::File::open(&event_config_path)?;
        let config: EventConfig = serde_yaml::from_reader(file)?;
        Ok(config)
    }

    pub async fn calculate_points(
        &self,
        challenge_metadata: &CtfChallengeMetadata,
        total_solves: u32,
        solve_index: u32,
        total_competitors: u32,
    ) -> Result<u32, Box<dyn std::error::Error>> {
        if let Some(points_fn) = &self.points_fn {
            // Use boa to execute the JS function
            let mut engine = create_boa_context();
            let flag_fn: Rc<Mutex<Option<JsFunction>>> = Rc::new(Mutex::new(None));
            let flag_fn_clone = flag_fn.clone();
            engine
                .register_global_builtin_callable(js_string!("setPointsFn"), 1, unsafe {
                    NativeFunction::from_closure(move |_this, args, _ctx| {
                        let fn_obj = args.first().and_then(|v| v.as_object());
                        if let Some(obj) = fn_obj {
                            let Some(func) = JsFunction::from_object(obj) else {
                                return Err(JsError::from(JsNativeError::typ().with_message(
                                    "setPointsFn expects a function as its first argument",
                                )));
                            };
                            let mut lock = flag_fn_clone.lock().unwrap();
                            *lock = Some(func);
                        } else {
                            return Err(JsError::from(JsNativeError::typ().with_message(
                                "setPointsFn expects a function as its first argument",
                            )));
                        }
                        Ok(JsValue::undefined())
                    })
                })
                .expect("Failed to register setPointsFn");
            engine.eval(Source::from_bytes(&points_fn))?;
            let points_function = {
                let mut lock = flag_fn.lock().unwrap();
                lock.take().ok_or("Points function not set")?
            };
            let challenge_metadata_js = challenge_metadata.try_into_js(&mut engine)?;
            let total_solves_js = js_value!(total_solves);
            let solve_index_js = js_value!(solve_index);
            let total_competitors_js = js_value!(total_competitors);
            let result = points_function.call(
                &JsValue::undefined(),
                &[
                    challenge_metadata_js,
                    total_solves_js,
                    solve_index_js,
                    total_competitors_js,
                ],
                &mut engine,
            )?;
            let points = result
                .as_i32()
                .ok_or("Points function did not return a number")?;
            Ok(points as u32)
        } else {
            // Default points calculation
            Ok(100)
        }
    }
}
