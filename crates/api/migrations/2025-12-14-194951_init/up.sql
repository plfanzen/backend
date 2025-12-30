-- SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

CREATE TYPE user_role AS ENUM ('PLAYER', 'AUTHOR', 'ADMIN');

CREATE TABLE teams (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    name VARCHAR NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    join_code VARCHAR UNIQUE
);

CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    username VARCHAR NOT NULL UNIQUE,
    display_name VARCHAR NOT NULL,
    password_hash VARCHAR NOT NULL,
    email VARCHAR NOT NULL UNIQUE,
    role user_role NOT NULL DEFAULT 'PLAYER',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    email_verified_at TIMESTAMPTZ,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    team_id UUID REFERENCES teams(id) ON DELETE SET NULL
);

CREATE INDEX idx_users_team_id ON users(team_id);

CREATE TABLE solves (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    user_id UUID REFERENCES users(id) ON DELETE CASCADE  NOT NULL,
    challenge_id VARCHAR NOT NULL,
    solved_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- In case a challenge author messes up the flag verification,
    -- we can check it manually if we still have the submitted flag stored.
    submitted_flag VARCHAR NOT NULL,
    UNIQUE (user_id, challenge_id)
);

CREATE INDEX idx_solves_user_id ON solves(user_id);
CREATE INDEX idx_solves_challenge_id ON solves(challenge_id);

-- As before, in case of flag verification issues, we store invalid submissions too.
-- This can also help in detecting brute-force attempts, or AI hallucinated flags.
CREATE TABLE invalid_submissions (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    user_id UUID REFERENCES users(id) ON DELETE CASCADE NOT NULL,
    challenge_id VARCHAR NOT NULL,
    submitted_flag VARCHAR NOT NULL,
    submitted_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_invalid_submissions_user_id ON invalid_submissions(user_id);
CREATE INDEX idx_invalid_submissions_challenge_id ON invalid_submissions(challenge_id);

CREATE TABLE team_join_requests (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    team_id UUID REFERENCES teams(id) ON DELETE CASCADE,
    requested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, team_id)
);

CREATE index idx_team_join_requests_user_id ON team_join_requests(user_id);
CREATE index idx_team_join_requests_team_id ON team_join_requests(team_id);

CREATE OR REPLACE FUNCTION clear_join_requests_on_team_join()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.team_id IS NOT NULL THEN
        DELETE FROM team_join_requests WHERE user_id = NEW.id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_clear_join_requests_on_team_join
AFTER UPDATE ON users
FOR EACH ROW
EXECUTE FUNCTION clear_join_requests_on_team_join();

CREATE TABLE team_invitations (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    team_id UUID REFERENCES teams(id) ON DELETE CASCADE,
    invited_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, team_id)
);
CREATE index idx_team_invitations_user_id ON team_invitations(user_id);
CREATE index idx_team_invitations_team_id ON team_invitations(team_id);

CREATE TABLE sessions (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL DEFAULT (NOW() + INTERVAL '7 days'),
    user_agent VARCHAR,
    ip_address INET,
    session_token VARCHAR NOT NULL UNIQUE
);

CREATE INDEX idx_sessions_user_id ON sessions(user_id);
CREATE INDEX idx_sessions_token ON sessions(session_token);

CREATE OR REPLACE FUNCTION cleanup_expired_sessions()
RETURNS VOID AS $$
BEGIN
    DELETE FROM sessions WHERE expires_at < NOW();
END;
$$ LANGUAGE plpgsql;

-- Try to create the cron extension if it doesn't exist, but don't fail if we can't
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'cron') THEN
        BEGIN
            CREATE EXTENSION pg_cron;
        EXCEPTION
            WHEN OTHERS THEN
                RAISE NOTICE 'Could not create pg_cron extension. Skipping cron job setup.';
        END;
    END IF;
    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'cron') THEN
        PERFORM cron.schedule('cleanup_expired_sessions', '0 * * * *', 'SELECT cleanup_expired_sessions();');
    END IF;
END $$;
