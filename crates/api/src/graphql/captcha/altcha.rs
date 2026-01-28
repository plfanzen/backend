use std::sync::LazyLock;

use crate::graphql::captcha::{CaptchaProvider, CaptchaProviderType};
use altcha_lib_rs::ChallengeOptions;
use chrono::Utc;

pub struct AltchaProvider;

const CAPTCHA_SECRET: LazyLock<Option<String>> =
    LazyLock::new(|| std::env::var("ALTCHA_SECRET_KEY").ok());

#[async_trait::async_trait]
impl CaptchaProvider for AltchaProvider {
    fn is_available(&self) -> bool {
        CAPTCHA_SECRET.is_some()
    }

    fn provider_type(&self) -> CaptchaProviderType {
        CaptchaProviderType::Altcha
    }

    async fn get_challenge(&self) -> juniper::FieldResult<serde_json::Value> {
        let Some(ref secret_key) = *CAPTCHA_SECRET else {
            return Err(juniper::FieldError::new(
                "Altcha CAPTCHA not configured",
                juniper::Value::null(),
            ));
        };
        let res = altcha_lib_rs::create_challenge(ChallengeOptions {
            hmac_key: secret_key,
            expires: Some(Utc::now() + chrono::Duration::minutes(5)),
            ..Default::default()
        })?;

        Ok(serde_json::to_value(res)?)
    }

    async fn verify_response(
        &self,
        _challenge: &str,
        response: &str,
    ) -> juniper::FieldResult<bool> {
        let Some(ref secret_key) = *CAPTCHA_SECRET else {
            return Err(juniper::FieldError::new(
                "Altcha CAPTCHA not configured",
                juniper::Value::null(),
            ));
        };
        let res = altcha_lib_rs::verify_json_solution(response, secret_key, true);
        if let Err(e) = &res {
            tracing::warn!("Altcha verification failed: {}", e);
        }
        Ok(res.is_ok())
    }
}
