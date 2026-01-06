// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;

use crate::repo::challenges::{dir_packer::safe_pack_challenge, metadata::CtfChallengeMetadata};
use tempfile::TempDir;

pub mod tera;

pub struct Challenge {
    pub metadata: CtfChallengeMetadata,
    pub compose: compose_spec::Compose,
    pub export: Option<Vec<u8>>,
}

pub async fn load_challenge_from_dir(
    chall_dir: &std::path::Path,
    actor: &str,
    is_export: bool,
) -> Result<Challenge, Box<dyn std::error::Error>> {
    // Process the challenge to a new temp dir
    let temp_dir = TempDir::new()?;
    let tmp_path = temp_dir.path().to_path_buf();
    let source_chall_dir = chall_dir.to_path_buf();
    let actor = actor.to_string();
    tokio::task::spawn_blocking(move || {
        tera::render_dir_recursively(&source_chall_dir, &tmp_path, &actor, is_export).map_err(|e| {
            format!(
                "Failed to render challenge directory {}: {}",
                source_chall_dir.to_string_lossy(),
                e
            )
        })
    })
    .await??;
    // Load docker-compose.yml from the temp dir
    let compose_path = temp_dir.path().join("docker-compose.yml");
    let compose_content = std::fs::read_to_string(&compose_path).map_err(|e| {
        format!(
            "Failed to read docker-compose.yml from {}: {}",
            compose_path.to_string_lossy(),
            e
        )
    })?;
    let compose: compose_spec::Compose = serde_yaml::from_str(&compose_content).map_err(|e| {
        format!(
            "Failed to parse docker-compose.yml from {}: {}",
            compose_path.to_string_lossy(),
            e
        )
    })?;
    let metadata = serde_yaml::from_value(
        compose
            .extensions
            .get("x-ctf-metadata")
            .ok_or(format!(
                "Missing ctf-metadata extension in docker-compose.yml at {}",
                compose_path.to_string_lossy()
            ))?
            .clone(),
    )
    .map_err(|e| {
        format!(
            "Failed to parse ctf-metadata from docker-compose.yml at {}: {}",
            compose_path.to_string_lossy(),
            e
        )
    })?;
    Ok(Challenge {
        metadata,
        compose,
        export: if is_export {
            Some(safe_pack_challenge(temp_dir.path()).map_err(move |e| {
                format!(
                    "Failed to pack challenge directory for challenge {}: {}",
                    chall_dir
                        .file_name()
                        .map(|n| n.to_string_lossy())
                        .unwrap_or_default(),
                    e
                )
            })?)
        } else {
            None
        },
    })
}

pub async fn load_challenges_from_repo(
    repo_path: &std::path::Path,
    actor: &str,
    is_export: bool,
) -> Result<HashMap<String, Challenge>, Box<dyn std::error::Error>> {
    let challenges_dir = repo_path.join("challs");
    let mut challenges = HashMap::new();

    if challenges_dir.is_dir() {
        for entry in std::fs::read_dir(challenges_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            match load_challenge_from_dir(&path, actor, is_export).await {
                Ok(challenge) => {
                    challenges.insert(
                        path.file_name().unwrap().to_string_lossy().to_string(),
                        challenge,
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to load challenge from directory {}: {}",
                        path.to_string_lossy(),
                        e
                    );
                }
            }
        }
    }

    Ok(challenges)
}

pub async fn load_challenge_from_repo(
    repo_path: &std::path::Path,
    challenge_id: &str,
    actor: &str,
    is_export: bool,
) -> Result<Challenge, Box<dyn std::error::Error>> {
    let challenge_dir = repo_path.join("challs").join(challenge_id);
    load_challenge_from_dir(&challenge_dir, actor, is_export).await
}
