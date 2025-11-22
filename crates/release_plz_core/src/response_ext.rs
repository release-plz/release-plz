use anyhow::Context as _;

pub(crate) trait ResponseExt {
    /// Better version of [`reqwest::Response::error_for_status`] that
    /// also captures the response body in the error message. It will most
    /// likely contain additional error details.
    async fn successful_status(self) -> anyhow::Result<reqwest::Response>;
}

impl ResponseExt for reqwest::Response {
    async fn successful_status(self) -> anyhow::Result<Self> {
        let Err(err) = self.error_for_status_ref() else {
            return Ok(self);
        };

        let mut body = self
            .text()
            .await
            .context("can't convert response body to text")?;

        // If the response is JSON, try to pretty-print it.
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
            body = format!("{json:#}");
        }

        Err(err).with_context(|| format!("Response body:\n{body}"))
    }
}
