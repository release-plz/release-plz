/// Client builder using the release-plz user agent, used
/// to identify release-plz to external http servers,
/// such as GitHub and crates.io.
pub fn http_client_builder() -> reqwest::ClientBuilder {
    let user_agent = format!("release-plz/{}", env!("CARGO_PKG_VERSION"));
    reqwest::Client::builder().user_agent(user_agent)
}
