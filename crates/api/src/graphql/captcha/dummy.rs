use serde_json::json;

use crate::graphql::captcha::{CaptchaProvider, CaptchaProviderType};

pub struct DummyProvider;

#[async_trait::async_trait]
impl CaptchaProvider for DummyProvider {
    fn is_available(&self) -> bool {
        true
    }

    fn provider_type(&self) -> CaptchaProviderType {
        CaptchaProviderType::Dummy
    }

    async fn get_challenge(&self) -> juniper::FieldResult<serde_json::Value> {
        Ok(json!({}))
    }

    async fn verify_response(
        &self,
        _challenge: &str,
        _response: &str,
    ) -> juniper::FieldResult<bool> {
        Ok(true)
    }
}
