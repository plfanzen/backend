use std::sync::LazyLock;

use serde_json::json;

use crate::graphql::captcha::CaptchaProvider;

pub struct CapProvider;

pub struct CaptchaCredentials {
    pub site_key: String,
    pub secret_key: String,
    pub instance_url: String,
}

const CAPTCHA_CREDENTIALS: LazyLock<Option<CaptchaCredentials>> = LazyLock::new(|| {
    let site_key = std::env::var("CAP_SITE_KEY").ok()?;
    let secret_key = std::env::var("CAP_SECRET_KEY").ok()?;
    let instance_url = std::env::var("CAP_INSTANCE_URL").ok()?;
    Some(CaptchaCredentials {
        site_key,
        secret_key,
        instance_url,
    })
});

#[async_trait::async_trait]
impl CaptchaProvider for CapProvider {
    fn is_available(&self) -> bool {
        CAPTCHA_CREDENTIALS.is_some()
    }

    fn provider_type(&self) -> crate::graphql::captcha::CaptchaProviderType {
        crate::graphql::captcha::CaptchaProviderType::Cap
    }

    async fn get_challenge(&self) -> juniper::FieldResult<serde_json::Value> {
        let Some(ref credentials) = *CAPTCHA_CREDENTIALS else {
            return Err(juniper::FieldError::new(
                "Cap is not configured",
                juniper::Value::null(),
            ));
        };
        Ok(json!({
            "site_key": credentials.site_key,
            "instance_url": credentials.instance_url,
        }))
    }

    async fn verify_response(
        &self,
        _challenge: &str,
        response: &str,
    ) -> juniper::FieldResult<bool> {
        let Some(ref credentials) = *CAPTCHA_CREDENTIALS else {
            return Err(juniper::FieldError::new(
                "Cap CAPTCHA not configured",
                juniper::Value::null(),
            ));
        };
        let resp = reqwest::Client::new()
            .post(format!(
                "https://{}/{}/siteverify",
                credentials.instance_url, credentials.site_key
            ))
            .header("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(5))
            .json(&serde_json::json!({
                "secret": credentials.secret_key,
                "response": response,
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            tracing::warn!("Cap verification HTTP error: {}", resp.status());
            return Ok(false);
        }

        let json: serde_json::Value = resp.json().await?;
        let success = json
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !success {
            tracing::info!("Cap verification failed: {:?}", json);
        }

        Ok(success)
    }
}
