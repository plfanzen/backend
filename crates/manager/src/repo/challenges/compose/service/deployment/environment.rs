// SPDX-FileCopyrightText: 2026 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::Path;

use crate::repo::challenges::compose::service::ComposeServiceError;

/// Processes environment variables from compose service configuration.
/// Returns a vector of Kubernetes EnvVar objects.
pub fn process_environment(
    svc: &compose_spec::Service,
    working_dir: &Path,
) -> Result<Vec<k8s_openapi::api::core::v1::EnvVar>, ComposeServiceError> {
    let mut env: Vec<_> = svc
        .environment
        .clone()
        .into_map()
        .map_err(|e| ComposeServiceError::Other(e.to_string()))?
        .into_iter()
        .map(|(k, v)| k8s_openapi::api::core::v1::EnvVar {
            name: k.to_string(),
            value: v.map(|val| val.to_string()),
            ..Default::default()
        })
        .collect();

    if let Some(env_file) = &svc.env_file {
        process_env_files(&mut env, env_file, working_dir)?;
    }

    Ok(env)
}

fn process_env_files(
    env: &mut Vec<k8s_openapi::api::core::v1::EnvVar>,
    env_file: &compose_spec::service::EnvFile,
    working_dir: &Path,
) -> Result<(), ComposeServiceError> {
    for file in env_file.clone().into_list() {
        let file = file.into_long();
        if file.path.is_absolute() {
            return Err(ComposeServiceError::EnvFileOutOfBounds(
                file.path.to_string_lossy().to_string(),
            ));
        }

        let abs_path = working_dir.join(file.path);
        match abs_path.canonicalize() {
            Err(e) => {
                if file.required {
                    return Err(ComposeServiceError::Other(format!(
                        "Failed to canonicalize env_file path {}: {}",
                        abs_path.to_string_lossy(),
                        e
                    )));
                } else {
                    continue;
                }
            }
            Ok(canonical_path) => {
                if !canonical_path.starts_with(working_dir) {
                    return Err(ComposeServiceError::EnvFileOutOfBounds(
                        canonical_path.to_string_lossy().to_string(),
                    ));
                }

                parse_env_file(env, &canonical_path, file.required)?;
            }
        }
    }

    Ok(())
}

fn parse_env_file(
    env: &mut Vec<k8s_openapi::api::core::v1::EnvVar>,
    canonical_path: &Path,
    required: bool,
) -> Result<(), ComposeServiceError> {
    let parsed = match dotenvy::from_path_iter(canonical_path) {
        Ok(iter) => iter,
        Err(e) => {
            if required {
                return Err(ComposeServiceError::EnvFileReadError(
                    canonical_path.to_string_lossy().to_string(),
                    match e {
                        dotenvy::Error::Io(io_err) => io_err,
                        // Should be unreachable, but handle just in case
                        other => std::io::Error::other(other.to_string()),
                    },
                ));
            } else {
                return Ok(());
            }
        }
    };

    for item in parsed {
        match item {
            Ok((key, value)) => {
                env.push(k8s_openapi::api::core::v1::EnvVar {
                    name: key,
                    value: Some(value),
                    ..Default::default()
                });
            }
            Err(e) => {
                if required {
                    return handle_env_parse_error(e, canonical_path);
                }
            }
        }
    }

    Ok(())
}

fn handle_env_parse_error(
    e: dotenvy::Error,
    canonical_path: &Path,
) -> Result<(), ComposeServiceError> {
    match e {
        dotenvy::Error::LineParse(line, line_no) => {
            Err(ComposeServiceError::EnvFileParseErrorDetailed(
                canonical_path.to_string_lossy().to_string(),
                line_no,
                line,
            ))
        }
        dotenvy::Error::Io(error) => Err(ComposeServiceError::EnvFileReadError(
            canonical_path.to_string_lossy().to_string(),
            error,
        )),
        dotenvy::Error::EnvVar(var_error) => Err(ComposeServiceError::EnvFileParseError(
            canonical_path.to_string_lossy().to_string(),
            var_error.to_string(),
        )),
        _ => Err(ComposeServiceError::EnvFileParseError(
            canonical_path.to_string_lossy().to_string(),
            e.to_string(),
        )),
    }
}
