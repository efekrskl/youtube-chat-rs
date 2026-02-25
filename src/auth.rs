use yup_oauth2::{InstalledFlowAuthenticator, InstalledFlowReturnMethod};

pub async fn auth() -> anyhow::Result<String> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .unwrap();

    let secret = yup_oauth2::read_application_secret("client_secret.json").await?;

    let auth = InstalledFlowAuthenticator::builder(secret, InstalledFlowReturnMethod::HTTPRedirect)
        .persist_tokens_to_disk("token_cache.json")
        .build()
        .await?;

    let scopes = &["https://www.googleapis.com/auth/youtube.readonly"];

    let access_token = auth.token(scopes).await?;
    
    let token = match access_token.token() {
        Some(t) => t,
        None => panic!("Couldn't get oauth token"),
    };
    
    Ok(token.to_string())
}
