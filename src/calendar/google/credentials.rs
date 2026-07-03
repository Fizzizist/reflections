use anyhow::{Context, Result};

pub struct Credentials {
    pub client_id: String,
    pub client_secret: String,
}

impl Credentials {
    pub fn from_env() -> Result<Self> {
        let client_id = option_env!("GOOGLE_CLIENT_ID")
            .context("binary was built without Google credentials — sync unavailable")?;
        let client_secret = option_env!("GOOGLE_CLIENT_SECRET")
            .context("binary was built without Google credentials — sync unavailable")?;
        Ok(Self {
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
        })
    }
}
