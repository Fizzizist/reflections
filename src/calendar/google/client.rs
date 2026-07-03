use crate::calendar::backend::CalendarBackend;
use crate::calendar::google::credentials::Credentials;
use crate::calendar::google::oauth::{self, Token};
use crate::calendar::types::CalendarEvent;
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Deserialize;

#[derive(Deserialize)]
struct CalendarListResponse {
    items: Vec<GoogleEvent>,
}

#[derive(Deserialize)]
struct GoogleEvent {
    summary: Option<String>,
    start: Option<GoogleEventTime>,
}

#[derive(Deserialize)]
struct GoogleEventTime {
    #[serde(rename = "dateTime")]
    date_time: Option<String>,
}

pub struct GoogleCalendarBackend {
    client: Client,
    credentials: Credentials,
}

impl GoogleCalendarBackend {
    pub fn new() -> Result<Self> {
        let credentials = Credentials::from_env()?;
        Ok(Self {
            client: Client::new(),
            credentials,
        })
    }

    async fn get_valid_token(&self) -> Result<Token> {
        if let Some(token) = oauth::load_token()? {
            if !token.is_expired() {
                return Ok(token);
            }
            if let Some(refresh) = &token.refresh_token
                && let Ok(new_token) = oauth::refresh_token(
                    &self.credentials.client_id,
                    &self.credentials.client_secret,
                    refresh,
                )
                .await
            {
                oauth::save_token(&new_token)?;
                return Ok(new_token);
            }
        }
        let token =
            oauth::authenticate(&self.credentials.client_id, &self.credentials.client_secret)
                .await?;
        Ok(token)
    }
}

#[async_trait]
impl CalendarBackend for GoogleCalendarBackend {
    async fn fetch_meetings(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<CalendarEvent>> {
        let token = self.get_valid_token().await?;

        let time_min = start.to_rfc3339();
        let time_max = end.to_rfc3339();

        let url = format!(
            "https://www.googleapis.com/calendar/v3/calendars/primary/events?timeMin={}&timeMax={}&singleEvents=true&orderBy=startTime",
            urlencoding::encode(&time_min),
            urlencoding::encode(&time_max)
        );

        let response = self
            .client
            .get(&url)
            .header(
                "Authorization",
                format!("{} {}", token.token_type, token.access_token),
            )
            .send()
            .await
            .context("failed to fetch calendar events")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "calendar API request failed with status {}: {}",
                status,
                body
            ));
        }

        let calendar_response: CalendarListResponse = response
            .json()
            .await
            .context("failed to parse calendar API response")?;

        let events: Vec<CalendarEvent> = calendar_response
            .items
            .into_iter()
            .filter_map(|event| {
                let summary = event.summary?;
                if summary.is_empty() {
                    return None;
                }
                let start_time = event.start?;
                let date_time_str = start_time.date_time?;
                let scheduled_at = DateTime::parse_from_rfc3339(&date_time_str)
                    .ok()?
                    .with_timezone(&Utc);
                Some(CalendarEvent {
                    name: summary,
                    scheduled_at,
                })
            })
            .collect();

        Ok(events)
    }
}
