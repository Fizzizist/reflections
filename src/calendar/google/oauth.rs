use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const REDIRECT_PORT: u16 = 8080;
const REDIRECT_URI: &str = "http://localhost:8080";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub token_type: String,
}

impl Token {
    pub fn is_expired(&self) -> bool {
        let threshold = Utc::now() + chrono::Duration::seconds(60);
        self.expires_at <= threshold
    }
}

fn parse_token_response(
    json: serde_json::Value,
    refresh_token_fallback: Option<String>,
) -> Result<Token> {
    let access_token = json["access_token"]
        .as_str()
        .context("missing access_token in response")?
        .to_string();
    let token_type = json["token_type"]
        .as_str()
        .context("missing token_type in response")?
        .to_string();
    let expires_in = json["expires_in"]
        .as_u64()
        .context("missing expires_in in response")?;
    let refresh_token = json["refresh_token"]
        .as_str()
        .map(String::from)
        .or(refresh_token_fallback);

    Ok(Token {
        access_token,
        refresh_token,
        expires_at: Utc::now() + chrono::Duration::seconds(expires_in as i64),
        token_type,
    })
}

pub fn token_path() -> Result<PathBuf> {
    let config_dir = dirs::config_dir().context("could not determine config directory")?;
    let reflections_dir = config_dir.join("reflections");
    Ok(reflections_dir.join("token.json"))
}

pub fn load_token_from(path: &Path) -> Result<Option<Token>> {
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read token file at {:?}", path))?;
    let token: Token =
        serde_json::from_str(&content).with_context(|| "failed to parse cached token")?;
    Ok(Some(token))
}

pub fn load_token() -> Result<Option<Token>> {
    load_token_from(&token_path()?)
}

pub fn save_token_to(path: &Path, token: &Token) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create config directory at {:?}", parent))?;
    }
    let content = serde_json::to_string_pretty(token).context("failed to serialize token")?;
    fs::write(path, content)
        .with_context(|| format!("failed to write token file at {:?}", path))?;
    Ok(())
}

pub fn save_token(token: &Token) -> Result<()> {
    save_token_to(&token_path()?, token)
}

pub async fn refresh_token(
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> Result<Token> {
    let client = reqwest::Client::new();
    let form_data = format!(
        "refresh_token={}&client_id={}&client_secret={}&grant_type=refresh_token",
        urlencoding::encode(refresh_token),
        urlencoding::encode(client_id),
        urlencoding::encode(client_secret),
    );
    let response = client
        .post("https://oauth2.googleapis.com/token")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(form_data)
        .send()
        .await
        .context("failed to send refresh token request")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<body unreadable>".to_string());
        return Err(anyhow::anyhow!(
            "token refresh failed with status {}: {}",
            status,
            body
        ));
    }

    let json: serde_json::Value = response
        .json()
        .await
        .context("failed to parse refresh token response")?;

    parse_token_response(json, Some(refresh_token.to_string()))
}

fn extract_query_param(url: &str, param: &str) -> Option<String> {
    let query_start = url.find('?')?;
    let query = &url[query_start + 1..];
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=')?;
        if key == param {
            return Some(value.to_string());
        }
    }
    None
}

fn extract_code_from_input(input: &str) -> String {
    if input.starts_with("http")
        && let Some(code) = extract_query_param(input, "code")
    {
        return code;
    }
    input.to_string()
}

pub fn build_auth_url(client_id: &str, state: &str) -> String {
    let scope = "https://www.googleapis.com/auth/calendar.readonly";
    format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&scope={}&access_type=offline&prompt=consent&response_type=code&state={}",
        urlencoding::encode(client_id),
        urlencoding::encode(REDIRECT_URI),
        urlencoding::encode(scope),
        urlencoding::encode(state)
    )
}

async fn wait_for_redirect_code(listener: TcpListener, expected_state: &str) -> Result<String> {
    let (mut stream, _) = listener
        .accept()
        .await
        .context("failed to accept OAuth redirect connection")?;

    let mut request = vec![0u8; 4096];
    let n = stream
        .read(&mut request)
        .await
        .context("failed to read OAuth redirect request")?;
    let request = String::from_utf8_lossy(&request[..n]);

    let request_line = request.lines().next().context("empty redirect request")?;
    let path = request_line
        .split_whitespace()
        .nth(1)
        .context("malformed redirect request line")?;

    let code = extract_query_param(path, "code").context("no code parameter in OAuth redirect")?;
    let state =
        extract_query_param(path, "state").context("no state parameter in OAuth redirect")?;

    if state != expected_state {
        return Err(anyhow::anyhow!(
            "OAuth state mismatch — possible CSRF attack"
        ));
    }

    let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n\
        <html><body><h2>Authorization received.</h2>\
        <p>You can close this tab and return to the terminal.</p></body></html>";
    stream
        .write_all(response.as_bytes())
        .await
        .context("failed to write redirect response to browser")?;
    stream.flush().await.context("failed to flush stream")?;

    Ok(code)
}

async fn read_code_from_stdin() -> Result<String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let stdin = io::stdin();
        let mut code = String::new();
        if stdin.lock().read_line(&mut code).is_ok() {
            let code = extract_code_from_input(code.trim());
            let _ = tx.send(code);
        }
    });
    let code = rx.await.context("failed to read from stdin thread")?;
    if code.is_empty() {
        return Err(anyhow::anyhow!("authorization code cannot be empty"));
    }
    Ok(code)
}

pub async fn exchange_code_for_token(
    client: &reqwest::Client,
    code: &str,
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
) -> Result<Token> {
    let form_data = format!(
        "code={}&client_id={}&client_secret={}&redirect_uri={}&grant_type=authorization_code",
        urlencoding::encode(code),
        urlencoding::encode(client_id),
        urlencoding::encode(client_secret),
        urlencoding::encode(redirect_uri),
    );
    let response = client
        .post("https://oauth2.googleapis.com/token")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(form_data)
        .send()
        .await
        .context("failed to send token exchange request")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<body unreadable>".to_string());
        return Err(anyhow::anyhow!(
            "token exchange failed with status {}: {}",
            status,
            body
        ));
    }

    let json: serde_json::Value = response
        .json()
        .await
        .context("failed to parse token exchange response")?;

    parse_token_response(json, None)
}

pub async fn authenticate(client_id: &str, client_secret: &str) -> Result<Token> {
    let state = uuid::Uuid::now_v7().to_string();
    let auth_url = build_auth_url(client_id, &state);

    let bind_addr = format!("127.0.0.1:{}", REDIRECT_PORT);
    let listener = TcpListener::bind(&bind_addr).await;

    // Interactive prompt — user needs to see URL and instructions
    writeln!(
        io::stderr(),
        "Opening browser for Google Calendar authorization..."
    )?;
    writeln!(io::stderr(), "If it doesn't open, visit:")?;
    writeln!(io::stderr(), "{}", auth_url)?;
    writeln!(io::stderr())?;

    if listener.is_ok() {
        writeln!(
            io::stderr(),
            "Waiting for browser redirect on http://localhost:{} ...",
            REDIRECT_PORT
        )?;
    }
    writeln!(
        io::stderr(),
        "If the redirect doesn't work (e.g. in a container or over SSH),"
    )?;
    writeln!(
        io::stderr(),
        "authorize in your browser and paste the 'code' param from the redirect URL here:"
    )?;
    writeln!(io::stderr())?;

    let _ = open::that(&auth_url);

    let timeout = std::time::Duration::from_secs(300);

    let code = if let Ok(listener) = listener {
        let server_code = wait_for_redirect_code(listener, &state);
        let stdin_code = read_code_from_stdin();

        tokio::select! {
            result = tokio::time::timeout(timeout, server_code) => {
                match result {
                    Ok(r) => r?,
                    Err(_) => anyhow::bail!("timed out waiting for OAuth redirect (5 minutes)"),
                }
            }
            result = tokio::time::timeout(timeout, stdin_code) => {
                match result {
                    Ok(r) => {
                        // Interactive prompt — user needs confirmation that manual code was accepted
                        writeln!(io::stderr(), "Using manually pasted code.")?;
                        r?
                    }
                    Err(_) => anyhow::bail!("timed out waiting for code input (5 minutes)"),
                }
            }
        }
    } else {
        writeln!(
            io::stderr(),
            "Could not bind port {} — manual code entry only.",
            REDIRECT_PORT
        )?;
        tokio::time::timeout(timeout, read_code_from_stdin())
            .await
            .map_err(|_| anyhow::anyhow!("timed out waiting for code input (5 minutes)"))?
            .context("failed to read authorization code from stdin")?
    };

    let client = reqwest::Client::new();
    let token =
        exchange_code_for_token(&client, &code, client_id, client_secret, REDIRECT_URI).await?;

    save_token(&token)?;
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use tempfile::NamedTempFile;

    #[test]
    fn token_serialization_round_trip() {
        let token = Token {
            access_token: "test_access".to_string(),
            refresh_token: Some("test_refresh".to_string()),
            expires_at: Utc::now() + Duration::hours(1),
            token_type: "Bearer".to_string(),
        };
        let json = serde_json::to_string(&token).expect("serialize failed");
        let deserialized: Token = serde_json::from_str(&json).expect("deserialize failed");
        assert_eq!(deserialized.access_token, token.access_token);
        assert_eq!(deserialized.refresh_token, token.refresh_token);
    }

    #[test]
    fn token_not_expired_when_future() {
        let token = Token {
            access_token: "test".to_string(),
            refresh_token: None,
            expires_at: Utc::now() + Duration::hours(1),
            token_type: "Bearer".to_string(),
        };
        assert!(!token.is_expired());
    }

    #[test]
    fn token_expired_when_past() {
        let token = Token {
            access_token: "test".to_string(),
            refresh_token: None,
            expires_at: Utc::now() - Duration::hours(1),
            token_type: "Bearer".to_string(),
        };
        assert!(token.is_expired());
    }

    #[test]
    fn token_expired_when_within_60_seconds() {
        let token = Token {
            access_token: "test".to_string(),
            refresh_token: None,
            expires_at: Utc::now() + Duration::seconds(30),
            token_type: "Bearer".to_string(),
        };
        assert!(token.is_expired());
    }

    #[test]
    fn extract_code_from_raw_code() {
        let result = extract_code_from_input("4/0AxKBa-xyz123");
        assert_eq!(result, "4/0AxKBa-xyz123");
    }

    #[test]
    fn extract_code_from_redirect_url() {
        let url = "http://localhost:8080/?code=4/0AxKBa-xyz123&scope=https://www.googleapis.com/auth/calendar.readonly";
        let result = extract_code_from_input(url);
        assert_eq!(result, "4/0AxKBa-xyz123");
    }

    #[test]
    fn extract_code_from_url_without_code_param() {
        let url = "http://localhost:8080/?error=access_denied";
        let result = extract_code_from_input(url);
        assert_eq!(result, url);
    }

    #[test]
    fn extract_query_param_finds_code() {
        let path = "/?code=4/0AxKBa-xyz&scope=foo";
        let result = extract_query_param(path, "code");
        assert_eq!(result, Some("4/0AxKBa-xyz".to_string()));
    }

    #[test]
    fn extract_query_param_returns_none_when_missing() {
        let path = "/?error=access_denied";
        let result = extract_query_param(path, "code");
        assert_eq!(result, None);
    }

    #[test]
    fn extract_query_param_returns_none_without_query_string() {
        let path = "/";
        let result = extract_query_param(path, "code");
        assert_eq!(result, None);
    }

    #[test]
    fn build_auth_url_includes_state() {
        let client_id = "test-client-id";
        let state = "test-state-123";
        let url = build_auth_url(client_id, state);
        assert!(url.contains("state=test-state-123"));
        assert!(url.contains("client_id=test-client-id"));
        assert!(url.contains("response_type=code"));
    }

    #[tokio::test]
    async fn wait_for_redirect_code_extracts_code_and_validates_state() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind failed");
        let addr = listener.local_addr().expect("addr failed");

        let server_task =
            tokio::spawn(
                async move { wait_for_redirect_code(listener, "expected-state-123").await },
            );

        let mut stream = tokio::net::TcpStream::connect(addr)
            .await
            .expect("connect failed");
        stream
            .write_all(
                b"GET /?code=4/0AxKBa-test123&state=expected-state-123&scope=calendar HTTP/1.1\r\nHost: localhost\r\n\r\n",
            )
            .await
            .expect("write failed");

        let code = server_task
            .await
            .expect("task panicked")
            .expect("extract failed");
        assert_eq!(code, "4/0AxKBa-test123");
    }

    #[tokio::test]
    async fn wait_for_redirect_code_rejects_mismatched_state() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind failed");
        let addr = listener.local_addr().expect("addr failed");

        let server_task =
            tokio::spawn(async move { wait_for_redirect_code(listener, "expected-state").await });

        let mut stream = tokio::net::TcpStream::connect(addr)
            .await
            .expect("connect failed");
        stream
            .write_all(
                b"GET /?code=4/0AxKBa-test&state=wrong-state HTTP/1.1\r\nHost: localhost\r\n\r\n",
            )
            .await
            .expect("write failed");

        let result = server_task.await.expect("task panicked");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("CSRF"));
    }

    #[tokio::test]
    async fn wait_for_redirect_code_returns_error_without_state_param() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind failed");
        let addr = listener.local_addr().expect("addr failed");

        let server_task =
            tokio::spawn(async move { wait_for_redirect_code(listener, "expected-state").await });

        let mut stream = tokio::net::TcpStream::connect(addr)
            .await
            .expect("connect failed");
        stream
            .write_all(b"GET /?code=4/0AxKBa-test HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .expect("write failed");

        let result = server_task.await.expect("task panicked");
        assert!(result.is_err());
    }

    #[test]
    fn parse_token_response_valid_with_refresh() {
        let json = serde_json::json!({
            "access_token": "test-access",
            "refresh_token": "test-refresh",
            "expires_in": 3600,
            "token_type": "Bearer"
        });
        let token = parse_token_response(json, None).expect("parse failed");
        assert_eq!(token.access_token, "test-access");
        assert_eq!(token.refresh_token, Some("test-refresh".to_string()));
        assert_eq!(token.token_type, "Bearer");
    }

    #[test]
    fn parse_token_response_valid_without_refresh() {
        let json = serde_json::json!({
            "access_token": "test-access",
            "expires_in": 3600,
            "token_type": "Bearer"
        });
        let token =
            parse_token_response(json, Some("old-refresh".to_string())).expect("parse failed");
        assert_eq!(token.access_token, "test-access");
        assert_eq!(token.refresh_token, Some("old-refresh".to_string()));
        assert_eq!(token.token_type, "Bearer");
    }

    #[test]
    fn parse_token_response_missing_access_token() {
        let json = serde_json::json!({
            "refresh_token": "test-refresh",
            "expires_in": 3600,
            "token_type": "Bearer"
        });
        let result = parse_token_response(json, None);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("access_token"));
    }

    #[test]
    fn parse_token_response_missing_expires_in() {
        let json = serde_json::json!({
            "access_token": "test-access",
            "refresh_token": "test-refresh",
            "token_type": "Bearer"
        });
        let result = parse_token_response(json, None);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("expires_in"));
    }

    #[test]
    fn parse_token_response_missing_token_type() {
        let json = serde_json::json!({
            "access_token": "test-access",
            "refresh_token": "test-refresh",
            "expires_in": 3600
        });
        let result = parse_token_response(json, None);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("token_type"));
    }

    #[test]
    fn save_token_to_then_load_token_from_round_trips() {
        let temp_file = NamedTempFile::new().expect("create temp file");
        let temp_path = temp_file.path();

        let token = Token {
            access_token: "test-access".to_string(),
            refresh_token: Some("test-refresh".to_string()),
            expires_at: Utc::now() + Duration::hours(1),
            token_type: "Bearer".to_string(),
        };

        save_token_to(temp_path, &token).expect("save failed");

        let loaded = load_token_from(temp_path)
            .expect("load failed")
            .expect("token not found");

        assert_eq!(loaded.access_token, token.access_token);
        assert_eq!(loaded.refresh_token, token.refresh_token);
        assert_eq!(loaded.token_type, token.token_type);
    }

    #[test]
    fn load_token_from_returns_none_when_file_missing() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let nonexistent_path = temp_dir.path().join("nonexistent_token.json");
        let result = load_token_from(&nonexistent_path).expect("load should not error");
        assert!(result.is_none());
    }

    #[test]
    fn load_token_from_returns_error_for_invalid_json() {
        let temp_file = NamedTempFile::new().expect("create temp file");
        std::fs::write(temp_file.path(), "not valid json").expect("write failed");
        let result = load_token_from(temp_file.path());
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("parse"));
    }
}
