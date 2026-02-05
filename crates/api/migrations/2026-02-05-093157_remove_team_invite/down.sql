-- This file should undo anything in `up.sql`
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
