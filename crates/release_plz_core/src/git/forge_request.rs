//! Retry forge requests without sending more traffic during a rate-limit cooldown.

use std::{
    sync::Mutex,
    time::{Duration, SystemTime},
};

use http::Extensions;
use reqwest::{Request, Response, ResponseBuilderExt as _, StatusCode, header::HeaderMap};
use reqwest_middleware::{Middleware, Next, Result};
use reqwest_retry::{
    DefaultRetryableStrategy, RetryDecision, RetryPolicy as _, Retryable, RetryableStrategy as _,
    policies::ExponentialBackoff,
};
use tokio::time::Instant;

use super::forge::ForgeType;

const MAX_RETRIES: u32 = 3;
const RETRY_BUDGET: Duration = Duration::from_secs(5 * 60);

/// An explicitly read-only POST, such as a GraphQL query (never a mutation).
#[derive(Clone, Debug)]
pub(super) struct ReadOnlyRequest;

/// `ClientWithMiddleware` shares middleware through an `Arc`, including in cloned clients.
#[derive(Debug)]
pub(super) struct ForgeRequestMiddleware {
    forge: ForgeType,
    cooldown: Mutex<Option<(Instant, Duration)>>,
    backoff: ExponentialBackoff,
}

impl ForgeRequestMiddleware {
    pub(super) fn new(forge: ForgeType) -> Self {
        Self {
            forge,
            cooldown: Mutex::new(None),
            backoff: ExponentialBackoff::builder().build_with_max_retries(MAX_RETRIES),
        }
    }

    fn extend_cooldown(&self, delay: Duration) {
        let mut cooldown = self.cooldown.lock().unwrap();
        if cooldown
            .is_none_or(|(started, duration)| duration.saturating_sub(started.elapsed()) < delay)
        {
            // Keep the duration separately: even an oversized server hint cannot overflow Instant.
            *cooldown = Some((Instant::now(), delay));
        }
    }

    async fn wait_for_cooldown(&self, deadline: Instant) -> Result<()> {
        loop {
            let remaining = self
                .cooldown
                .lock()
                .unwrap()
                .map(|(started, duration)| duration.saturating_sub(started.elapsed()))
                .unwrap_or_default();
            if remaining.is_zero() {
                return Ok(());
            }
            if remaining > deadline.saturating_duration_since(Instant::now()) {
                return Err(reqwest_middleware::Error::Middleware(anyhow::anyhow!(
                    "Git forge rate-limit cooldown exceeds the five-minute retry budget; retry later"
                )));
            }
            tokio::time::sleep(remaining).await;
            // Another in-flight request may have extended the shared cooldown while we slept.
        }
    }

    async fn rate_limit(&self, response: Response) -> Result<(Response, bool)> {
        let status = response.status();
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Ok((response, true));
        }
        if status != StatusCode::FORBIDDEN || self.forge != ForgeType::Github {
            return Ok((response, false));
        }
        if response.headers().contains_key("retry-after") || quota_exhausted(response.headers()) {
            return Ok((response, true));
        }

        // GitHub secondary limits can return 403 with only an explanatory JSON message.
        // Preserve the status, headers, URL and body for callers handling ordinary permission errors.
        let mut buffered = http::Response::builder()
            .status(status)
            .version(response.version());
        *buffered.headers_mut().unwrap() = response.headers().clone();
        *buffered.extensions_mut().unwrap() = response.extensions().clone();
        buffered = buffered.url(response.url().clone());
        let body = response.bytes().await?;
        let limited = serde_json::from_slice::<serde_json::Value>(&body)
            .ok()
            .and_then(|body| body.get("message")?.as_str().map(str::to_ascii_lowercase))
            .is_some_and(|message| {
                message.contains("secondary rate limit")
                    || message.contains("api rate limit exceeded")
                    || message.contains("abuse detection mechanism")
            });
        Ok((buffered.body(body).expect("valid response").into(), limited))
    }
}

#[async_trait::async_trait]
impl Middleware for ForgeRequestMiddleware {
    async fn handle(
        &self,
        request: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> Result<Response> {
        let deadline = Instant::now() + RETRY_BUDGET;
        let safe_to_repeat =
            request.method().is_safe() || extensions.get::<ReadOnlyRequest>().is_some();
        for retries in 0..=MAX_RETRIES {
            self.wait_for_cooldown(deadline).await?;
            let duplicate = request.try_clone().ok_or_else(|| {
                reqwest_middleware::Error::Middleware(anyhow::anyhow!(
                    "Git forge request body cannot be cloned for retries"
                ))
            })?;
            let result = next.clone().run(duplicate, extensions).await;
            let (result, limited) = match result {
                Ok(response) => {
                    let (response, limited) = self.rate_limit(response).await?;
                    (Ok(response), limited)
                }
                Err(error) => (Err(error), false),
            };
            let retryable = limited
                || (safe_to_repeat
                    && matches!(
                        DefaultRetryableStrategy.handle(&result),
                        Some(Retryable::Transient)
                    ));
            if !retryable {
                return result;
            }
            let backoff = match self.backoff.should_retry(SystemTime::now(), retries) {
                RetryDecision::Retry { execute_after } => execute_after
                    .duration_since(SystemTime::now())
                    .unwrap_or_default(),
                RetryDecision::DoNotRetry => Duration::ZERO,
            };
            if limited {
                let response = result.as_ref().expect("rate limit has an HTTP response");
                let delay = retry_after(response.headers(), SystemTime::now())
                    // GitHub recommends at least a minute when a secondary limit has no retry hint.
                    .unwrap_or_else(|| Duration::from_secs(60 * 2_u64.pow(retries)))
                    .saturating_add(backoff);
                self.extend_cooldown(delay);
                tracing::warn!(?delay, retries, "Git forge rate limited requests");
            }
            if retries == MAX_RETRIES {
                return result;
            }
            if !limited {
                if backoff > deadline.saturating_duration_since(Instant::now()) {
                    return result;
                }
                tracing::warn!(?backoff, retries, "Retrying failed Git forge read");
                tokio::time::sleep(backoff).await;
            }
            // Explicit throttling means the operation was rejected, so even writes can be retried.
            // Ambiguous failures (timeouts, 5xx, connection loss) are only retried for reads.
        }
        unreachable!("the final attempt always returns")
    }
}

fn quota_exhausted(headers: &HeaderMap) -> bool {
    headers
        .get("x-ratelimit-remaining")
        .is_some_and(|remaining| remaining == "0")
}

/// Server hints are minimum delays. If both hints are present, respect the later one.
fn retry_after(headers: &HeaderMap, now: SystemTime) -> Option<Duration> {
    let retry_after = headers.get("retry-after").and_then(|value| {
        let value = value.to_str().ok()?;
        value
            .parse::<u64>()
            .ok()
            .map(Duration::from_secs)
            .or_else(|| {
                httpdate::parse_http_date(value)
                    .ok()
                    .map(|date| date.duration_since(now).unwrap_or_default())
            })
    });
    let reset = quota_exhausted(headers)
        .then(|| {
            let seconds = headers
                .get("x-ratelimit-reset")?
                .to_str()
                .ok()?
                .parse()
                .ok()?;
            Some(
                Duration::from_secs(seconds).saturating_sub(
                    now.duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default(),
                ),
            )
        })
        .flatten();
    retry_after.into_iter().chain(reset).max()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use reqwest::Method;
    use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
    use reqwest_retry::Jitter;
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::any};

    use super::*;

    fn middleware(forge: ForgeType) -> ForgeRequestMiddleware {
        ForgeRequestMiddleware {
            // Zero jitter/backoff makes HTTP tests fast; server hints and fallback still apply.
            backoff: ExponentialBackoff::builder()
                .retry_bounds(Duration::ZERO, Duration::ZERO)
                .jitter(Jitter::None)
                .build_with_max_retries(MAX_RETRIES),
            ..ForgeRequestMiddleware::new(forge)
        }
    }

    fn client(forge: ForgeType) -> ClientWithMiddleware {
        ClientBuilder::new(reqwest::Client::new())
            .with(middleware(forge))
            .build()
    }

    #[test]
    fn retry_hints_are_minimum_waits() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
        for (retry, remaining, reset, expected) in [
            ("10", "1", "1000000020", Some(10)),
            ("10", "0", "1000000020", Some(20)),
            ("30", "0", "1000000020", Some(30)),
            ("invalid", "0", "1000000020", Some(20)),
            ("invalid", "1", "1000000020", None),
            ("invalid", "0", "invalid", None),
            (
                "invalid",
                "0",
                "18446744073709551615",
                Some(u64::MAX - 1_000_000_000),
            ),
            ("0", "0", "1", Some(0)),
            ("Sun, 09 Sep 2001 01:47:00 GMT", "1", "0", Some(20)),
            ("Sun, 09 Sep 2001 01:46:00 GMT", "1", "0", Some(0)),
        ] {
            let headers = HeaderMap::from_iter([
                (reqwest::header::RETRY_AFTER, retry.parse().unwrap()),
                (
                    reqwest::header::HeaderName::from_static("x-ratelimit-remaining"),
                    remaining.parse().unwrap(),
                ),
                (
                    reqwest::header::HeaderName::from_static("x-ratelimit-reset"),
                    reset.parse().unwrap(),
                ),
            ]);
            assert_eq!(
                retry_after(&headers, now),
                expected.map(Duration::from_secs)
            );
        }
    }

    #[tokio::test]
    async fn retries_throttled_get_and_post_requests() {
        for method in [Method::GET, Method::POST] {
            for status in [429, 403] {
                let server = MockServer::start().await;
                Mock::given(any())
                    .respond_with(ResponseTemplate::new(status).insert_header("retry-after", "0"))
                    .up_to_n_times(1)
                    .expect(1)
                    .with_priority(1)
                    .mount(&server)
                    .await;
                Mock::given(any())
                    .respond_with(ResponseTemplate::new(200))
                    .expect(1)
                    .with_priority(2)
                    .mount(&server)
                    .await;
                let response = client(ForgeType::Github)
                    .request(method.clone(), server.uri())
                    .send()
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::OK);
            }
        }
    }

    #[tokio::test]
    async fn github_primary_limit_uses_reset_header() {
        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(
                ResponseTemplate::new(403)
                    .insert_header("x-ratelimit-remaining", "0")
                    .insert_header("x-ratelimit-reset", "1"),
            )
            .up_to_n_times(1)
            .expect(1)
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .with_priority(2)
            .mount(&server)
            .await;
        assert_eq!(
            client(ForgeType::Github)
                .get(server.uri())
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
    }

    #[tokio::test]
    async fn permission_errors_keep_their_status_body_and_url() {
        for forge in [ForgeType::Github, ForgeType::Gitlab, ForgeType::Gitea] {
            let server = MockServer::start().await;
            let body = r#"{"message":"Resource not accessible by integration"}"#;
            let mut template = ResponseTemplate::new(403).set_body_string(body);
            if forge != ForgeType::Github {
                // A Retry-After on another forge's 403 does not establish a GitHub rate limit.
                template = template.insert_header("retry-after", "0");
            }
            Mock::given(any())
                .respond_with(template)
                .expect(1)
                .mount(&server)
                .await;
            let response = client(forge).get(server.uri()).send().await.unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
            assert_eq!(response.url().as_str(), format!("{}/", server.uri()));
            assert_eq!(response.text().await.unwrap(), body);
        }
    }

    #[tokio::test]
    async fn throttling_retries_are_bounded_for_every_forge() {
        for forge in [ForgeType::Github, ForgeType::Gitlab, ForgeType::Gitea] {
            let server = MockServer::start().await;
            Mock::given(any())
                .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
                .expect(4)
                .mount(&server)
                .await;
            let response = client(forge).get(server.uri()).send().await.unwrap();
            assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        }
    }

    #[tokio::test]
    async fn transient_failures_only_retry_safe_reads() {
        for (method, read_only, requests) in [
            (Method::GET, false, 4),
            (Method::POST, true, 4),
            (Method::POST, false, 1),
            (Method::PATCH, false, 1),
        ] {
            let server = MockServer::start().await;
            Mock::given(any())
                .respond_with(ResponseTemplate::new(503))
                .expect(requests)
                .mount(&server)
                .await;
            let mut request = client(ForgeType::Gitlab).request(method, server.uri());
            if read_only {
                request = request.with_extension(ReadOnlyRequest);
            }
            assert_eq!(request.send().await.unwrap().status(), 503);
        }
    }

    /// An in-memory transport lets Tokio's paused clock test real retry scheduling without IO.
    #[derive(Clone, Debug)]
    struct Responses {
        statuses: Arc<Mutex<Vec<(Instant, String)>>>,
        status: StatusCode,
        retry_after: Option<&'static str>,
        body: &'static str,
    }

    #[async_trait::async_trait]
    impl Middleware for Responses {
        async fn handle(
            &self,
            request: Request,
            _: &mut Extensions,
            _: Next<'_>,
        ) -> Result<Response> {
            let mut statuses = self.statuses.lock().unwrap();
            statuses.push((Instant::now(), request.url().path().to_owned()));
            let mut response = http::Response::builder().status(self.status);
            if let Some(retry_after) = self.retry_after {
                response = response.header("retry-after", retry_after);
            }
            Ok(response.body(self.body).unwrap().into())
        }
    }

    fn timed_client(
        status: StatusCode,
        retry_after: Option<&'static str>,
        body: &'static str,
    ) -> (ClientWithMiddleware, Responses) {
        let responses = Responses {
            statuses: Arc::default(),
            status,
            retry_after,
            body,
        };
        let client = ClientBuilder::new(reqwest::Client::new())
            .with(middleware(ForgeType::Github))
            .with(responses.clone())
            .build();
        (client, responses)
    }

    #[tokio::test(start_paused = true)]
    async fn secondary_limit_without_headers_backs_off_for_at_least_a_minute() {
        let (client, responses) = timed_client(
            StatusCode::FORBIDDEN,
            None,
            r#"{"message":"You have exceeded a secondary rate limit."}"#,
        );
        let start = Instant::now();
        let error = client.get("http://forge.test/").send().await.unwrap_err();
        assert!(error.to_string().contains("retry budget"));
        let times: Vec<_> = responses
            .statuses
            .lock()
            .unwrap()
            .iter()
            .map(|(time, _)| time.duration_since(start).as_secs())
            .collect();
        assert_eq!(times, [0, 60, 180]);
    }

    #[tokio::test(start_paused = true)]
    async fn cooldown_is_shared_by_clones_and_checked_before_each_attempt() {
        let (client, responses) = timed_client(StatusCode::TOO_MANY_REQUESTS, Some("10"), "");
        let start = Instant::now();
        let first = tokio::spawn({
            let client = client.clone();
            async move { client.get("http://forge.test/first").send().await }
        });
        tokio::task::yield_now().await;
        assert_eq!(responses.statuses.lock().unwrap().len(), 1);
        let second =
            tokio::spawn(async move { client.get("http://forge.test/second").send().await });
        tokio::task::yield_now().await;
        assert_eq!(responses.statuses.lock().unwrap().len(), 1);
        first.await.unwrap().unwrap();
        second.await.unwrap().unwrap();
        let statuses = responses.statuses.lock().unwrap();
        assert_eq!(statuses.len(), 8);
        assert!(
            statuses
                .windows(2)
                .all(|pair| pair[1].0 - pair[0].0 >= Duration::from_secs(10))
        );
        let second_start = statuses
            .iter()
            .find(|(_, path)| path == "/second")
            .unwrap()
            .0;
        assert!(second_start - start >= Duration::from_secs(10));
    }

    #[tokio::test(start_paused = true)]
    async fn excessive_retry_hints_fail_without_retrying_early_or_overflowing() {
        for retry_after in ["3600", "18446744073709551615"] {
            let (client, responses) =
                timed_client(StatusCode::TOO_MANY_REQUESTS, Some(retry_after), "");
            let start = Instant::now();
            for _ in 0..2 {
                let error = client.get("http://forge.test/").send().await.unwrap_err();
                assert!(error.to_string().contains("retry budget"));
            }
            assert_eq!(responses.statuses.lock().unwrap().len(), 1);
            assert_eq!(start.elapsed(), Duration::ZERO);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn shorter_cooldowns_do_not_replace_longer_ones() {
        let middleware = middleware(ForgeType::Github);
        middleware.extend_cooldown(Duration::from_secs(30));
        middleware.extend_cooldown(Duration::from_secs(10));
        let start = Instant::now();
        middleware
            .wait_for_cooldown(start + RETRY_BUDGET)
            .await
            .unwrap();
        assert_eq!(start.elapsed(), Duration::from_secs(30));
    }
}
