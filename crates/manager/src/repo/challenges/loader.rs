// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;

mod tera;

pub async fn load_challenge_from_dir(
    chall_dir: &std::path::Path,
    actor: &str,
) -> Result<crate::repo::challenges::manifest::ChallengeYml, Box<dyn std::error::Error>> {
    let chal_yml = if chall_dir.join("chal.yml.jinja").is_file() {
        tera::process_template(&chall_dir.join("chal.yml.jinja"), actor).map_err(|e| {
            format!(
                "Failed to process templated challenge manifest {}: {}",
                chall_dir.join("chal.yml.jinja").to_string_lossy(),
                e
            )
        })?
    } else {
        std::fs::read_to_string(chall_dir.join("chal.yml")).map_err(|e| {
            format!(
                "Failed to read challenge manifest {}: {}",
                chall_dir.join("chal.yml").to_string_lossy(),
                e
            )
        })?
    };
    let challenge: crate::repo::challenges::manifest::ChallengeYml =
        serde_yaml::from_str(&chal_yml).map_err(|e| {
            format!(
                "Failed to parse challenge manifest {}: {}",
                chall_dir.join("chal.yml").to_string_lossy(),
                e
            )
        })?;
    Ok(challenge)
}

pub async fn load_challenges_from_repo(
    repo_path: &std::path::Path,
    actor: &str,
) -> Result<
    HashMap<String, crate::repo::challenges::manifest::ChallengeYml>,
    Box<dyn std::error::Error>,
> {
    let challenges_dir = repo_path.join("challs");
    let mut challenges = HashMap::new();

    if challenges_dir.is_dir() {
        for entry in std::fs::read_dir(challenges_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            match load_challenge_from_dir(&path, actor).await {
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
) -> Result<crate::repo::challenges::manifest::ChallengeYml, Box<dyn std::error::Error>> {
    let challenge_dir = repo_path.join("challs").join(challenge_id);
    load_challenge_from_dir(&challenge_dir, actor).await
}
