// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub mod service;
pub mod volume;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::repo::challenges::metadata::CtfChallengeMetadata;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChallengeYml {
    pub services: HashMap<String, service::ChallengeService>,
    pub volumes: HashMap<String, volume::ChallengeVolume>,
    pub metadata: CtfChallengeMetadata,
}
