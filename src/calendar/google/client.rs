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

fn parse_single_event(event: GoogleEvent) -> Option<CalendarEvent> {
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
}

fn parse_calendar_events(items: Vec<GoogleEvent>) -> Vec<CalendarEvent> {
    items.into_iter().filter_map(parse_single_event).collect()
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
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<body unreadable>".to_string());
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

        let events = parse_calendar_events(calendar_response.items);

        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn parse_single_event_with_valid_data() {
        let event = GoogleEvent {
            summary: Some("Team Meeting".to_string()),
            start: Some(GoogleEventTime {
                date_time: Some("2024-01-15T10:00:00Z".to_string()),
            }),
        };
        let result = parse_single_event(event);
        assert!(result.is_some());
        let cal_event = result.expect("expected Some");
        assert_eq!(cal_event.name, "Team Meeting");
        assert_eq!(
            cal_event.scheduled_at,
            Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap()
        );
    }

    #[test]
    fn parse_single_event_without_summary() {
        let event = GoogleEvent {
            summary: None,
            start: Some(GoogleEventTime {
                date_time: Some("2024-01-15T10:00:00Z".to_string()),
            }),
        };
        let result = parse_single_event(event);
        assert!(result.is_none());
    }

    #[test]
    fn parse_single_event_with_empty_summary() {
        let event = GoogleEvent {
            summary: Some("".to_string()),
            start: Some(GoogleEventTime {
                date_time: Some("2024-01-15T10:00:00Z".to_string()),
            }),
        };
        let result = parse_single_event(event);
        assert!(result.is_none());
    }

    #[test]
    fn parse_single_event_without_start() {
        let event = GoogleEvent {
            summary: Some("Team Meeting".to_string()),
            start: None,
        };
        let result = parse_single_event(event);
        assert!(result.is_none());
    }

    #[test]
    fn parse_single_event_all_day_event() {
        let event = GoogleEvent {
            summary: Some("All Day Event".to_string()),
            start: Some(GoogleEventTime { date_time: None }),
        };
        let result = parse_single_event(event);
        assert!(result.is_none());
    }

    #[test]
    fn parse_single_event_with_invalid_datetime() {
        let event = GoogleEvent {
            summary: Some("Team Meeting".to_string()),
            start: Some(GoogleEventTime {
                date_time: Some("garbage".to_string()),
            }),
        };
        let result = parse_single_event(event);
        assert!(result.is_none());
    }

    #[test]
    fn parse_calendar_events_filters_and_maps() {
        let events = vec![
            GoogleEvent {
                summary: Some("Valid Event".to_string()),
                start: Some(GoogleEventTime {
                    date_time: Some("2024-01-15T10:00:00Z".to_string()),
                }),
            },
            GoogleEvent {
                summary: None,
                start: Some(GoogleEventTime {
                    date_time: Some("2024-01-15T11:00:00Z".to_string()),
                }),
            },
            GoogleEvent {
                summary: Some("Another Valid".to_string()),
                start: Some(GoogleEventTime {
                    date_time: Some("2024-01-15T12:00:00Z".to_string()),
                }),
            },
            GoogleEvent {
                summary: Some("".to_string()),
                start: Some(GoogleEventTime {
                    date_time: Some("2024-01-15T13:00:00Z".to_string()),
                }),
            },
        ];
        let result = parse_calendar_events(events);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "Valid Event");
        assert_eq!(result[1].name, "Another Valid");
    }

    #[test]
    fn parse_calendar_events_empty() {
        let events: Vec<GoogleEvent> = vec![];
        let result = parse_calendar_events(events);
        assert!(result.is_empty());
    }
}
