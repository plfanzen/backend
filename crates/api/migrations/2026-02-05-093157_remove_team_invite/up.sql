DROP TABLE IF EXISTS team_invitations;
DROP INDEX IF EXISTS idx_team_invitations_team_id;
DROP INDEX IF EXISTS idx_team_invitations_user_id;

DROP TRIGGER IF EXISTS trg_clear_join_requests_on_team_join ON users;
DROP FUNCTION IF EXISTS clear_join_requests_on_team_join();

DROP TABLE IF EXISTS team_join_requests;
DROP INDEX IF EXISTS idx_team_join_requests_team_id;
DROP INDEX IF EXISTS idx_team_join_requests_user_id;
