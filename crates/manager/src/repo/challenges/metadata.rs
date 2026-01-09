// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::{Arc, Mutex};

use boa_engine::{
    JsError, JsNativeError, JsValue, NativeFunction, Source, js_string, js_value,
    object::builtins::JsFunction, value::TryIntoJs,
};
use serde::{Deserialize, Serialize};

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::js::create_boa_context;

fn json_into_js(
    value: &serde_json::Value,
    context: &mut boa_engine::Context,
) -> boa_engine::JsResult<boa_engine::JsValue> {
    JsValue::from_json(value, context)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum FlagValidator {
    String {
        flag: String,
    },
    JsFunction {
        /// JS code that runs setFlagValidationFunction((flag) => boolean)
        flag_validation_fn: String,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, TryIntoJs)]
pub struct CtfChallengeMetadata {
    /// Name of the challenge
    pub name: String,
    /// Authors of the challenge
    pub authors: Vec<String>,
    /// Description of the challenge in Markdown format
    pub description_md: String,
    #[serde(flatten)]
    #[boa(skip)]
    pub flag_validator: FlagValidator,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    // Path to attached files
    #[serde(default)]
    pub attachments: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_time: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<u64>,
    /// Whether to automatically expose source code + docker images + docker-compose for this challenge
    #[serde(default)]
    pub auto_publish_src: bool,
    pub difficulty: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_pvc_size: Option<String>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    #[boa(into_js_with = "json_into_js")]
    pub additional_metadata: serde_json::Value,
}

impl CtfChallengeMetadata {
    pub fn check_flag(&self, input_flag: &str) -> Result<bool, Box<dyn std::error::Error>> {
        match &self.flag_validator {
            FlagValidator::String { flag } => Ok(flag == input_flag),
            FlagValidator::JsFunction { flag_validation_fn } => {
                let mut engine = create_boa_context();
                let flag_fn: Arc<Mutex<Option<JsFunction>>> = Arc::new(Mutex::new(None));
                let flag_fn_clone = flag_fn.clone();
                engine
                    .register_global_builtin_callable(
                        js_string!("setFlagValidationFunction"),
                        1,
                        unsafe {
                            NativeFunction::from_closure(move |_this, args, _ctx| {
                                let fn_obj = args.get(0).and_then(|v| v.as_object());
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
                        },
                    )
                    .expect("Failed to register setFlagValidationFunction");
                engine.eval(Source::from_bytes(&flag_validation_fn))?;
                let flag_validation_function = {
                    let mut lock = flag_fn.lock().unwrap();
                    lock.take().ok_or("Flag validation function not set")?
                };
                let result = flag_validation_function.call(
                    &JsValue::undefined(),
                    &[js_value!(js_string!(input_flag))],
                    &mut engine,
                )?;
                let success = result
                    .as_boolean()
                    .ok_or("Flag validation function did not return a boolean")?;
                Ok(success)
            }
        }
    }

    pub fn get_password(&self, actor: &str, instance_id: &str, password_id: &str) -> String {
        let hmac_key = if let Ok(env_key) = std::env::var("HMAC_SECRET_KEY") {
            env_key.into_bytes()
        } else {
            tracing::warn!(
                "HMAC_SECRET_KEY environment variable not set, using challenge data only for password generation. This is insecure!"
            );
            match self.flag_validator {
                FlagValidator::String { ref flag } => flag.clone().into_bytes(),
                FlagValidator::JsFunction {
                    ref flag_validation_fn,
                } => flag_validation_fn.clone().into_bytes(),
            }
        };
        let mut mac =
            Hmac::<Sha256>::new_from_slice(&hmac_key).expect("HMAC can take key of any size");
        mac.update(actor.as_bytes());
        mac.update(instance_id.as_bytes());
        mac.update(password_id.as_bytes());
        let result = mac.finalize();
        let code_bytes = result.into_bytes();
        let hex_str = code_bytes
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();
        hex_str[..16].to_string()
    }
}
