mod gitea_client;
mod gitea_new;

#[derive(Debug)]
pub struct GiteaUser {
    username: String,
    password: String,
}

impl GiteaUser {
    pub fn email(&self) -> String {
        format!("{}@example.com", self.username)
    }
}

#[derive(Debug)]
pub struct GiteaContext {
    pub user: GiteaUser,
    pub token: String,
    /// Repository name.
    pub repo: String,
    client: reqwest::Client,
}

/// Repository name where gitea stores the registry index.
pub const CARGO_INDEX_REPO: &str = "_cargo-index";

fn gitea_endpoint(endpoint: &str) -> String {
    let api_url = format!("http://{}/api/v1", gitea_address());
    format!("{api_url}/{endpoint}")
}

pub fn gitea_address() -> &'static str {
    "localhost:3000"
}
