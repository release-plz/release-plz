use std::fmt::Display;

use reqwest::{Response, StatusCode};

use crate::response_ext::ResponseExt;

/// A forge operation failed, optionally because the server denied access.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// The server returned HTTP 401 or 403.
    #[error(transparent)]
    Authorization(anyhow::Error),
    /// A failure that does not indicate an authorization problem.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl GitError {
    /// Add context without erasing the error variant.
    pub(crate) fn context(self, context: impl Display + Send + Sync + 'static) -> Self {
        match self {
            Self::Authorization(error) => Self::Authorization(error.context(context)),
            Self::Other(error) => Self::Other(error.context(context)),
        }
    }

    /// Preserve the HTTP status classification and response body at the API boundary.
    pub(crate) async fn check_response(response: Response) -> Result<Response, Self> {
        let status = response.status();
        response
            .successful_status()
            .await
            .map_err(|error| match status {
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Self::Authorization(error),
                _ => Self::Other(error),
            })
    }
}

impl From<reqwest::Error> for GitError {
    fn from(error: reqwest::Error) -> Self {
        match error.status() {
            Some(StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) => {
                Self::Authorization(error.into())
            }
            _ => Self::Other(error.into()),
        }
    }
}

impl From<reqwest_middleware::Error> for GitError {
    fn from(error: reqwest_middleware::Error) -> Self {
        match error {
            reqwest_middleware::Error::Reqwest(error) => error.into(),
            reqwest_middleware::Error::Middleware(error) => Self::Other(error),
        }
    }
}
