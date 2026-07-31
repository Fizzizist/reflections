use chrono::{DateTime, Utc};
use serde::Serialize;
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventType {
    TodoItemCreated,
    TodoItemStatusChanged,
    MeetingCreated,
    ReflectionCreated,
    NoteCreated,
    SummaryCreated,
    ReflectionUpdated,
    SummaryUpdated,
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventType::TodoItemCreated => write!(f, "TODO_ITEM_CREATED"),
            EventType::TodoItemStatusChanged => write!(f, "TODO_ITEM_STATUS_CHANGED"),
            EventType::MeetingCreated => write!(f, "MEETING_CREATED"),
            EventType::ReflectionCreated => write!(f, "REFLECTION_CREATED"),
            EventType::NoteCreated => write!(f, "NOTE_CREATED"),
            EventType::SummaryCreated => write!(f, "SUMMARY_CREATED"),
            EventType::ReflectionUpdated => write!(f, "REFLECTION_UPDATED"),
            EventType::SummaryUpdated => write!(f, "SUMMARY_UPDATED"),
        }
    }
}

impl FromStr for EventType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "TODO_ITEM_CREATED" => Ok(EventType::TodoItemCreated),
            "TODO_ITEM_STATUS_CHANGED" => Ok(EventType::TodoItemStatusChanged),
            "MEETING_CREATED" => Ok(EventType::MeetingCreated),
            "REFLECTION_CREATED" => Ok(EventType::ReflectionCreated),
            "REFLECTION_UPDATED" => Ok(EventType::ReflectionUpdated),
            "NOTE_CREATED" => Ok(EventType::NoteCreated),
            "SUMMARY_CREATED" => Ok(EventType::SummaryCreated),
            "SUMMARY_UPDATED" => Ok(EventType::SummaryUpdated),
            other => Err(anyhow::anyhow!("invalid EventType: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Event {
    pub event_id: Uuid,
    pub entity_id: Uuid,
    pub event_type: EventType,
    pub metadata: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Serialize for EventType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reflection_created_display() {
        assert_eq!(
            EventType::ReflectionCreated.to_string(),
            "REFLECTION_CREATED"
        );
    }

    #[test]
    fn reflection_created_from_str() {
        assert_eq!(
            "REFLECTION_CREATED"
                .parse::<EventType>()
                .expect("parse failed"),
            EventType::ReflectionCreated
        );
    }

    #[test]
    fn note_created_display() {
        assert_eq!(EventType::NoteCreated.to_string(), "NOTE_CREATED");
    }

    #[test]
    fn note_created_from_str() {
        assert_eq!(
            "NOTE_CREATED".parse::<EventType>().expect("parse failed"),
            EventType::NoteCreated
        );
    }

    #[test]
    fn summary_created_display_and_from_str() {
        assert_eq!(EventType::SummaryCreated.to_string(), "SUMMARY_CREATED");
        assert_eq!(
            "SUMMARY_CREATED"
                .parse::<EventType>()
                .expect("parse failed"),
            EventType::SummaryCreated
        );
    }

    #[test]
    fn reflection_updated_display_and_from_str() {
        assert_eq!(
            EventType::ReflectionUpdated.to_string(),
            "REFLECTION_UPDATED"
        );
        assert_eq!(
            "REFLECTION_UPDATED"
                .parse::<EventType>()
                .expect("parse failed"),
            EventType::ReflectionUpdated
        );
    }

    #[test]
    fn summary_updated_display_and_from_str() {
        assert_eq!(EventType::SummaryUpdated.to_string(), "SUMMARY_UPDATED");
        assert_eq!(
            "SUMMARY_UPDATED"
                .parse::<EventType>()
                .expect("parse failed"),
            EventType::SummaryUpdated
        );
    }
}
