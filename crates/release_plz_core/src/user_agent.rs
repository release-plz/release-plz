/// User agents to identify release-plz to external http servers,
/// such as GitHub and crates.io.
pub fn user_agent() -> String {
    format!("release-plz/{}", env!("CARGO_PKG_VERSION"))
}
