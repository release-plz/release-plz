pub mod cmd;
mod fake_utils;
pub mod gitea;
pub mod package;
mod reqwest_utils;
pub mod test_context;

pub const TEST_REGISTRY: &str = "test-registry";

pub fn today() -> String {
    // The changelogs specify the release date in UTC.
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}
