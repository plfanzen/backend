// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

mod api {
    tonic::include_proto!("plfanzen_ctf");
}

mod challenges;
mod repository;

pub use api::challenges_service_server::ChallengesServiceServer;
pub use api::repository_service_server::RepositoryServiceServer;
pub use challenges::ChallengeManager;
pub use repository::RepoManager;
