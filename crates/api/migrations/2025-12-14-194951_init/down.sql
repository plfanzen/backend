-- SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

-- Remove the cron job for cleanup_expired_sessions, but don't fail if cron is not installed
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'cron') THEN
        PERFORM cron.unschedule('cleanup_expired_sessions');
    END IF;
END $$;

-- Drop the cleanup function
DROP FUNCTION IF EXISTS cleanup_expired_sessions();

-- Drop tables in reverse order (to handle foreign key dependencies)
DROP TABLE IF EXISTS sessions;
DROP INDEX IF EXISTS idx_sessions_token;
DROP INDEX IF EXISTS idx_sessions_user_id;

DROP TABLE IF EXISTS team_invitations;
DROP INDEX IF EXISTS idx_team_invitations_team_id;
DROP INDEX IF EXISTS idx_team_invitations_user_id;

-- Drop trigger and function for clearing join requests
DROP TRIGGER IF EXISTS trg_clear_join_requests_on_team_join ON users;
DROP FUNCTION IF EXISTS clear_join_requests_on_team_join();

DROP TABLE IF EXISTS team_join_requests;
DROP INDEX IF EXISTS idx_team_join_requests_team_id;
DROP INDEX IF EXISTS idx_team_join_requests_user_id;

DROP TABLE IF EXISTS invalid_submissions;
DROP INDEX IF EXISTS idx_invalid_submissions_challenge_id;
DROP INDEX IF EXISTS idx_invalid_submissions_user_id;

DROP TABLE IF EXISTS solves;
DROP INDEX IF EXISTS idx_solves_challenge_id;
DROP INDEX IF EXISTS idx_solves_user_id;

DROP INDEX IF EXISTS idx_users_team_id;
DROP TABLE IF EXISTS users;

DROP TABLE IF EXISTS teams;

-- Drop the custom enum type
DROP TYPE IF EXISTS user_role;
