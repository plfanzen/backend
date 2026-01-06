use tonic::Code;

use crate::graphql::Context;

pub async fn export_challenge(
    ctx: Context,
    challenge_id: String,
) -> Result<Vec<u8>, (u16, String)> {
    let auth = ctx.require_authentication().map_err(|e| (401, format!("Authentication required: {:?}", e)))?;
    let actor = auth.actor();

    let mut challenges_client = ctx.challenges_client();

    let resp = challenges_client
        .export_challenge(crate::manager_api::ExportChallengeRequest {
            actor,
            challenge_id,
            require_release: auth.role < crate::graphql::UserRole::Author,
        })
        .await;

    match resp {
        Ok(response) => {
            let response = response.into_inner();
            Ok(response.challenge_archive)
        }
        Err(status) => {
            return Err((
                if status.code() == Code::PermissionDenied {
                    403
                } else if status.code() == Code::NotFound {
                    404
                } else if status.code() == Code::InvalidArgument {
                    400
                } else {
                    500
                },
                format!("Failed to export challenge: {}", status.message()),
            ));
        }
    }
}

pub async fn retrieve_file(
    ctx: Context,
    challenge_id: String,
    filename: String,
) -> Result<Vec<u8>, (u16, String)> {
    let auth = ctx.require_authentication().map_err(|e| (401, format!("Authentication required: {:?}", e)))?;
    let actor = auth.actor();

    let mut challenges_client = ctx.challenges_client();

    let resp = challenges_client
        .retrieve_file(crate::manager_api::RetrieveFileRequest {
            actor,
            challenge_id,
            filename,
            require_release: auth.role < crate::graphql::UserRole::Author,
        })
        .await;

    match resp {
        Ok(response) => {
            let response = response.into_inner();
            Ok(response.file_content)
        }
        Err(status) => {
            return Err((
                if status.code() == Code::PermissionDenied {
                    403
                } else if status.code() == Code::NotFound {
                    404
                } else if status.code() == Code::InvalidArgument {
                    400
                } else {
                    500
                },
                format!("Failed to retrieve file: {}", status.message()),
            ));
        }
    }
}
