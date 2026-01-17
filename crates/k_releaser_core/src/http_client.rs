/// Client builder using the k-releaser user agent, used
/// to identify release-plz to external http servers,
/// such as GitHub and crates.io.
pub fn http_client_builder() -> reqwest::ClientBuilder {
    let user_agent = format!("k-releaser/{}", env!("CARGO_PKG_VERSION"));
    reqwest::Client::builder().user_agent(user_agent)
}
