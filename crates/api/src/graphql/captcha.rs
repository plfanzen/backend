use std::sync::LazyLock;

mod altcha;
mod cap;
mod dummy;

static CAPTCHA_PROVIDER: LazyLock<Box<dyn CaptchaProvider + Send + Sync>> = LazyLock::new(|| {
    for provider in [
        Box::new(altcha::AltchaProvider) as Box<dyn CaptchaProvider + Send + Sync>,
        Box::new(cap::CapProvider) as Box<dyn CaptchaProvider + Send + Sync>,
    ] {
        if provider.is_available() {
            tracing::info!("Using CAPTCHA provider: {:?}", provider.provider_type());
            return provider;
        }
    }
    Box::new(dummy::DummyProvider)
});

#[derive(juniper::GraphQLEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptchaProviderType {
    Altcha,
    Cap,
    Dummy,
}

#[async_trait::async_trait]
pub trait CaptchaProvider {
    fn is_available(&self) -> bool;
    fn provider_type(&self) -> CaptchaProviderType;
    async fn get_challenge(&self) -> juniper::FieldResult<serde_json::Value>;
    async fn verify_response(&self, challenge: &str, response: &str) -> juniper::FieldResult<bool>;
}

#[derive(juniper::GraphQLObject)]
pub struct CaptchaChallenge {
    pub provider_type: CaptchaProviderType,
    pub challenge: String,
}

pub async fn get_captcha_challenge(
    _context: &super::Context,
) -> juniper::FieldResult<CaptchaChallenge> {
    let challenge = CAPTCHA_PROVIDER.get_challenge().await?;
    let captcha_challenge = CaptchaChallenge {
        provider_type: CAPTCHA_PROVIDER.provider_type(),
        challenge: serde_json::to_string(&challenge)?,
    };
    Ok(captcha_challenge)
}

pub async fn verify_captcha_response(
    challenge: &str,
    response: &str,
) -> juniper::FieldResult<bool> {
    CAPTCHA_PROVIDER.verify_response(challenge, response).await
}
