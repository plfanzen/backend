// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::error::Error;

use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

pub mod models;
pub mod schema;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

pub fn run_migrations(
    connection: &mut impl MigrationHarness<diesel::pg::Pg>,
) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    connection.run_pending_migrations(MIGRATIONS)?;

    Ok(())
}
