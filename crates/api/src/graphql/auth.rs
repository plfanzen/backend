use std::time::Duration;

use base64::prelude::*;
use ed25519_dalek::{
    Signature, SignatureError, SigningKey, Verifier, VerifyingKey, ed25519::signature::Signer,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;
use uuid::Uuid;

use crate::db::models::UserRole;

#[derive(Serialize, Deserialize)]
struct JwtHeader {
    alg: String,
    typ: String,
}

#[derive(Serialize, Deserialize)]
#[serde(bound = "Inner: Serialize + DeserializeOwned")]
pub struct JwtPayload<Inner: DeserializeOwned> {
    #[serde(flatten)]
    pub custom_fields: Inner,
    pub sub: uuid::Uuid,
    #[serde(default)]
    pub aud: Vec<String>,
    exp: usize,
    iat: usize,
    nbf: usize,
}

impl<Inner: DeserializeOwned> JwtPayload<Inner> {
    pub fn new_with_duration(
        sub: uuid::Uuid,
        aud: Vec<String>,
        custom_fields: Inner,
        valid_duration: Duration,
    ) -> Self {
        let current_time = chrono::Utc::now().timestamp() as usize;
        Self {
            sub,
            aud,
            custom_fields,
            iat: current_time,
            nbf: current_time,
            exp: current_time + valid_duration.as_secs() as usize,
        }
    }

    pub fn new_with_exp_ts(
        sub: uuid::Uuid,
        aud: Vec<String>,
        custom_fields: Inner,
        expires_at: usize,
    ) -> Self {
        let current_time = chrono::Utc::now().timestamp() as usize;
        Self {
            sub,
            aud,
            custom_fields,
            iat: current_time,
            nbf: current_time,
            exp: expires_at,
        }
    }

    pub fn is_valid_now(&self) -> bool {
        let current_time = chrono::Utc::now().timestamp() as usize;
        current_time >= self.nbf && current_time <= self.exp
    }
}

#[derive(Serialize, Deserialize)]
pub struct AuthJwtPayload {
    pub role: UserRole,
    pub username: String,
    pub team_slug: Option<String>,
    pub team_id: Option<Uuid>,
}

#[derive(Serialize, Deserialize)]
pub struct RefreshJwtPayload {
    pub jti: String,
    pub session_id: uuid::Uuid,
}

#[derive(Error, Debug)]
pub enum JwtValidationError {
    #[error("Invalid JWT format")]
    InvalidFormat,
    #[error("Base64 decoding error: {0}")]
    Base64DecodingError(#[from] base64::DecodeError),
    #[error("Unsupported JWT algorithm: {0}")]
    UnsupportedAlgorithm(String),
    #[error("Invalid JWT signature: {0}")]
    InvalidSignature(#[from] SignatureError),
    #[error("JWT parsing error: {0}")]
    ParsingError(#[from] serde_json::Error),
    #[error("JWT is not valid at the current time")]
    InvalidTime,
}

#[derive(Error, Debug)]
pub enum JwtGenerationError {
    #[error("JWT signing error: {0}")]
    SigningError(#[from] SignatureError),
    #[error("JWT serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

/// Validate a JWT and its signature
fn validate_jwt(token: &str, verifying_key: &VerifyingKey) -> Result<(), JwtValidationError> {
    let segments: Vec<&str> = token.split('.').collect();
    if segments.len() != 3 {
        return Err(JwtValidationError::InvalidFormat);
    }
    let header_segment = segments[0];
    let payload_segment = segments[1];
    let signature_segment = segments[2];

    let decoded_header = BASE64_URL_SAFE.decode(header_segment)?;
    let header = serde_json::from_slice::<JwtHeader>(&decoded_header)?;
    if header.alg != "EdDSA" {
        return Err(JwtValidationError::UnsupportedAlgorithm(header.alg));
    }

    let signature_bytes = BASE64_URL_SAFE.decode(signature_segment)?;
    let signature = Signature::from_slice(&signature_bytes)?;
    let signed_data = format!("{}.{}", header_segment, payload_segment);
    verifying_key.verify(signed_data.as_bytes(), &signature)?;
    Ok(())
}

pub fn parse_and_validate_jwt<T: DeserializeOwned + Serialize>(
    token: &str,
    verifying_key: &VerifyingKey,
) -> Result<JwtPayload<T>, JwtValidationError> {
    validate_jwt(token, verifying_key)?;

    let segments: Vec<&str> = token.split('.').collect();
    let payload_segment = segments[1];

    let decoded_payload = BASE64_URL_SAFE.decode(payload_segment)?;
    let payload: JwtPayload<T> = serde_json::from_slice(&decoded_payload)?;

    if !payload.is_valid_now() {
        return Err(JwtValidationError::InvalidTime);
    }

    Ok(payload)
}

pub fn generate_jwt<T: Serialize>(
    payload: &T,
    signing_key: &SigningKey,
) -> Result<String, JwtGenerationError> {
    let header = JwtHeader {
        alg: "EdDSA".to_string(),
        typ: "JWT".to_string(),
    };
    let header_json = serde_json::to_vec(&header)?;
    let payload_json = serde_json::to_vec(payload)?;

    let header_segment = BASE64_URL_SAFE.encode(header_json);
    let payload_segment = BASE64_URL_SAFE.encode(payload_json);
    let signing_input = format!("{}.{}", header_segment, payload_segment);

    let signature: Signature = signing_key.try_sign(signing_input.as_bytes())?;
    let signature_segment = BASE64_URL_SAFE.encode(signature.to_bytes());

    Ok(format!(
        "{}.{}.{}",
        header_segment, payload_segment, signature_segment
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    #[test]
    fn test_jwt_generation_and_validation() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = VerifyingKey::from(&signing_key);

        let inner = AuthJwtPayload {
            role: UserRole::Player,
            username: "testuser".to_string(),
            team_slug: None,
            team_id: None,
        };

        let jwt_payload = JwtPayload::new_with_duration(
            uuid::Uuid::now_v7(),
            Vec::new(),
            inner,
            Duration::from_secs(3600),
        );

        let token = generate_jwt(&jwt_payload, &signing_key).expect("Failed to generate JWT");
        let parsed_payload: JwtPayload<AuthJwtPayload> =
            parse_and_validate_jwt(&token, &verifying_key).expect("Failed to parse JWT");

        assert_eq!(parsed_payload.sub, jwt_payload.sub);
        assert_eq!(
            parsed_payload.custom_fields.role,
            jwt_payload.custom_fields.role
        );
    }

    #[test]
    fn test_jwt_invalid_signature() {
        let signing_key = SigningKey::generate(&mut OsRng);

        let another_signing_key = SigningKey::generate(&mut OsRng);
        let another_verifying_key = VerifyingKey::from(&another_signing_key);
        let inner = AuthJwtPayload {
            role: UserRole::Player,
            username: "testuser".to_string(),
            team_slug: None,
            team_id: None,
        };
        let jwt_payload = JwtPayload::new_with_duration(
            uuid::Uuid::now_v7(),
            Vec::new(),
            inner,
            Duration::from_secs(3600),
        );
        let token = generate_jwt(&jwt_payload, &signing_key).expect("Failed to generate JWT");
        let result = parse_and_validate_jwt::<AuthJwtPayload>(&token, &another_verifying_key);
        assert!(matches!(
            result,
            Err(JwtValidationError::InvalidSignature(_))
        ));
    }

    #[test]
    fn test_jwt_invalid_time() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = VerifyingKey::from(&signing_key);
        let inner = AuthJwtPayload {
            role: UserRole::Player,
            username: "testuser".to_string(),
            team_slug: None,
            team_id: None,
        };
        let jwt_payload = JwtPayload::new_with_duration(
            uuid::Uuid::now_v7(),
            Vec::new(),
            inner,
            Duration::from_secs(0),
        ); // Expired immediately
        let token = generate_jwt(&jwt_payload, &signing_key).expect("Failed to generate JWT");
        std::thread::sleep(std::time::Duration::from_secs(1)); // Wait to ensure token is expired
        let result = parse_and_validate_jwt::<AuthJwtPayload>(&token, &verifying_key);
        assert!(matches!(result, Err(JwtValidationError::InvalidTime)));
    }
}
