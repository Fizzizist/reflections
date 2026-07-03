use anyhow::Result;
use chrono::{DateTime, Utc};
use turso::Connection;
use turso::transaction::Transaction;
use uuid::Uuid;

use crate::models::event::Event;
use crate::models::event::EventType;
use crate::repositories::parse_timestamp;
use std::str::FromStr;

const SELECT_COLUMNS: &str =
    "event_id, entity_id, event_type, metadata, created_at, updated_at FROM event";

fn row_to_event(row: &turso::Row) -> Result<Event> {
    let event_id_str: String = row.get(0)?;
    let event_id = Uuid::parse_str(&event_id_str)?;
    let entity_id_str: String = row.get(1)?;
    let entity_id = Uuid::parse_str(&entity_id_str)?;
    let event_type_str: String = row.get(2)?;
    let event_type = EventType::from_str(&event_type_str)?;
    let metadata: String = row.get(3)?;
    let created_str: String = row.get(4)?;
    let created_at = parse_timestamp(&created_str)?;
    let updated_str: String = row.get(5)?;
    let updated_at = parse_timestamp(&updated_str)?;

    Ok(Event {
        event_id,
        entity_id,
        event_type,
        metadata,
        created_at,
        updated_at,
    })
}

pub struct EventFilter {
    pub entity_id: Option<Uuid>,
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
}

impl EventFilter {
    pub fn new() -> Self {
        Self {
            entity_id: None,
            start: None,
            end: None,
        }
    }

    pub fn entity_id(mut self, entity_id: Uuid) -> Self {
        self.entity_id = Some(entity_id);
        self
    }

    pub fn start(mut self, start: DateTime<Utc>) -> Self {
        self.start = Some(start);
        self
    }

    pub fn end(mut self, end: DateTime<Utc>) -> Self {
        self.end = Some(end);
        self
    }

    fn build_where_clause(&self) -> (String, Vec<String>) {
        let mut conditions = Vec::new();
        let mut params = Vec::new();

        if let Some(entity_id) = self.entity_id {
            conditions.push("entity_id = ?");
            params.push(entity_id.to_string());
        }
        if let Some(start) = self.start {
            conditions.push("created_at >= ?");
            params.push(start.format("%Y-%m-%d %H:%M:%S").to_string());
        }
        if let Some(end) = self.end {
            conditions.push("created_at < ?");
            params.push(end.format("%Y-%m-%d %H:%M:%S").to_string());
        }

        let clause = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };

        (clause, params)
    }
}

pub async fn find(conn: &Connection, filter: &EventFilter) -> Result<Vec<Event>> {
    let (where_clause, params) = filter.build_where_clause();
    let sql = format!(
        "SELECT {}{} ORDER BY created_at ASC",
        SELECT_COLUMNS, where_clause
    );

    let mut rows = conn.query(&sql, params).await?;

    let mut events = Vec::new();
    while let Some(row) = rows.next().await? {
        events.push(row_to_event(&row)?);
    }

    Ok(events)
}

#[expect(dead_code)]
pub async fn find_one(conn: &Connection, filter: &EventFilter) -> Result<Option<Event>> {
    let (where_clause, params) = filter.build_where_clause();
    let sql = format!(
        "SELECT {}{} ORDER BY created_at ASC LIMIT 1",
        SELECT_COLUMNS, where_clause
    );

    let mut rows = conn.query(&sql, params).await?;
    let row = rows.next().await?;
    while rows.next().await.is_ok_and(|r| r.is_some()) {}
    if let Some(row) = row {
        return Ok(Some(row_to_event(&row)?));
    }
    Ok(None)
}

pub async fn list_by_date_range(
    conn: &Connection,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<Event>> {
    let filter = EventFilter::new().start(start).end(end);
    find(conn, &filter).await
}

pub async fn insert(
    tx: &Transaction<'_>,
    entity_id: Uuid,
    event_type: &EventType,
    metadata: &str,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    let sql = "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)";
    tx.execute(
        sql,
        (
            Uuid::now_v7().to_string(),
            entity_id.to_string(),
            event_type.to_string(),
            metadata.to_string(),
            now.clone(),
            now,
        ),
    )
    .await?;
    Ok(())
}

pub async fn delete_by_entity_id(tx: &Transaction<'_>, entity_id: Uuid) -> Result<()> {
    let sql = "DELETE FROM event WHERE entity_id = ?";
    tx.execute(sql, [entity_id.to_string()]).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;
    use chrono::Duration;

    async fn setup() -> turso::Connection {
        let db = turso::Builder::new_local(":memory:")
            .experimental_custom_types(true)
            .build()
            .await
            .expect("db build failed");
        let conn = db.connect().expect("db connect failed");
        schema::init_schema(&conn)
            .await
            .expect("schema init failed");
        conn
    }

    #[tokio::test]
    async fn insert_event() {
        let mut conn = setup().await;
        let entity_id = Uuid::now_v7();

        let tx = conn.transaction().await.expect("tx begin failed");
        insert(&tx, entity_id, &EventType::TodoItemCreated, "{}")
            .await
            .expect("insert failed");
        tx.commit().await.expect("commit failed");

        let mut rows = conn
            .query(
                "SELECT event_id, entity_id, event_type FROM event WHERE entity_id = ?",
                [entity_id.to_string()],
            )
            .await
            .expect("query failed");

        let row = rows
            .next()
            .await
            .expect("row fetch failed")
            .expect("event not found");

        let event_type_str = row
            .get_value(2)
            .expect("value extraction failed")
            .as_text()
            .expect("expected text")
            .to_string();
        assert_eq!(event_type_str, "TODO_ITEM_CREATED");
    }

    #[tokio::test]
    async fn event_filter_by_entity_id_returns_only_matching() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let entity1 = Uuid::now_v7();
        let entity2 = Uuid::now_v7();
        let now = Utc::now();

        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), entity1.to_string(), "TODO_ITEM_CREATED", "{}", now.format("%Y-%m-%d %H:%M:%S").to_string(), now.format("%Y-%m-%d %H:%M:%S").to_string()),
        ).await.expect("insert e1 failed");

        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), entity2.to_string(), "TODO_ITEM_CREATED", "{}", now.format("%Y-%m-%d %H:%M:%S").to_string(), now.format("%Y-%m-%d %H:%M:%S").to_string()),
        ).await.expect("insert e2 failed");

        tx.commit().await.expect("commit failed");

        let filter = EventFilter::new().entity_id(entity1);
        let events = find(&conn, &filter).await.expect("find failed");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].entity_id, entity1);
    }

    #[tokio::test]
    async fn event_filter_by_entity_id_orders_oldest_first() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let entity_id = Uuid::now_v7();
        let t1 = Utc::now() - Duration::minutes(30);
        let t2 = Utc::now() - Duration::minutes(20);
        let t3 = Utc::now() - Duration::minutes(10);

        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), entity_id.to_string(), "TODO_ITEM_CREATED", "{}", t3.format("%Y-%m-%d %H:%M:%S").to_string(), t3.format("%Y-%m-%d %H:%M:%S").to_string()),
        ).await.expect("insert e1 failed");

        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), entity_id.to_string(), "TODO_ITEM_CREATED", "{}", t1.format("%Y-%m-%d %H:%M:%S").to_string(), t1.format("%Y-%m-%d %H:%M:%S").to_string()),
        ).await.expect("insert e2 failed");

        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), entity_id.to_string(), "TODO_ITEM_CREATED", "{}", t2.format("%Y-%m-%d %H:%M:%S").to_string(), t2.format("%Y-%m-%d %H:%M:%S").to_string()),
        ).await.expect("insert e3 failed");

        tx.commit().await.expect("commit failed");

        let filter = EventFilter::new().entity_id(entity_id);
        let events = find(&conn, &filter).await.expect("find failed");

        assert_eq!(events.len(), 3);
        assert_eq!(
            events[0].created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            t1.format("%Y-%m-%d %H:%M:%S").to_string()
        );
        assert_eq!(
            events[1].created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            t2.format("%Y-%m-%d %H:%M:%S").to_string()
        );
        assert_eq!(
            events[2].created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            t3.format("%Y-%m-%d %H:%M:%S").to_string()
        );
    }

    #[tokio::test]
    async fn event_filter_by_entity_id_empty_for_nonexistent() {
        let conn = setup().await;
        let nonexistent = Uuid::now_v7();

        let filter = EventFilter::new().entity_id(nonexistent);
        let events = find(&conn, &filter).await.expect("find failed");

        assert_eq!(events.len(), 0);
    }

    #[tokio::test]
    async fn event_filter_by_date_range_returns_events_in_range() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let now = Utc::now();
        let start = now - Duration::hours(1);
        let end = now + Duration::hours(1);

        let entity_id = Uuid::now_v7();
        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), entity_id.to_string(), "TODO_ITEM_CREATED", "{}", (start + Duration::minutes(10)).format("%Y-%m-%d %H:%M:%S").to_string(), (start + Duration::minutes(10)).format("%Y-%m-%d %H:%M:%S").to_string()),
        ).await.expect("insert e1 failed");

        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), entity_id.to_string(), "TODO_ITEM_STATUS_CHANGED", "{}", (start + Duration::minutes(30)).format("%Y-%m-%d %H:%M:%S").to_string(), (start + Duration::minutes(30)).format("%Y-%m-%d %H:%M:%S").to_string()),
        ).await.expect("insert e2 failed");

        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), entity_id.to_string(), "MEETING_CREATED", "{}", (start - Duration::hours(2)).format("%Y-%m-%d %H:%M:%S").to_string(), (start - Duration::hours(2)).format("%Y-%m-%d %H:%M:%S").to_string()),
        ).await.expect("insert e3 failed");

        tx.commit().await.expect("commit failed");

        let filter = EventFilter::new().start(start).end(end);
        let events = find(&conn, &filter).await.expect("find failed");

        assert_eq!(events.len(), 2);
    }

    #[tokio::test]
    async fn event_filter_by_date_range_orders_oldest_first() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let now = Utc::now();
        let entity_id = Uuid::now_v7();

        let t1 = now - Duration::minutes(30);
        let t2 = now - Duration::minutes(20);
        let t3 = now - Duration::minutes(10);

        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), entity_id.to_string(), "TODO_ITEM_CREATED", "{}", t3.format("%Y-%m-%d %H:%M:%S").to_string(), t3.format("%Y-%m-%d %H:%M:%S").to_string()),
        ).await.expect("insert e1 failed");

        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), entity_id.to_string(), "TODO_ITEM_CREATED", "{}", t1.format("%Y-%m-%d %H:%M:%S").to_string(), t1.format("%Y-%m-%d %H:%M:%S").to_string()),
        ).await.expect("insert e2 failed");

        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), entity_id.to_string(), "TODO_ITEM_CREATED", "{}", t2.format("%Y-%m-%d %H:%M:%S").to_string(), t2.format("%Y-%m-%d %H:%M:%S").to_string()),
        ).await.expect("insert e3 failed");

        tx.commit().await.expect("commit failed");

        let filter = EventFilter::new()
            .start(now - Duration::hours(1))
            .end(now + Duration::hours(1));
        let events = find(&conn, &filter).await.expect("find failed");

        assert_eq!(events.len(), 3);
        assert_eq!(
            events[0].created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            t1.format("%Y-%m-%d %H:%M:%S").to_string()
        );
        assert_eq!(
            events[1].created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            t2.format("%Y-%m-%d %H:%M:%S").to_string()
        );
        assert_eq!(
            events[2].created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            t3.format("%Y-%m-%d %H:%M:%S").to_string()
        );
    }

    #[tokio::test]
    async fn event_filter_by_date_range_empty() {
        let conn = setup().await;

        let start = Utc::now() - Duration::hours(1);
        let end = Utc::now() + Duration::hours(1);

        let filter = EventFilter::new().start(start).end(end);
        let events = find(&conn, &filter).await.expect("find failed");

        assert_eq!(events.len(), 0);
    }

    #[tokio::test]
    async fn event_filter_combined_entity_id_and_date_range() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let entity1 = Uuid::now_v7();
        let entity2 = Uuid::now_v7();
        let now = Utc::now();
        let start = now - Duration::minutes(30);
        let end = now + Duration::minutes(30);

        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), entity1.to_string(), "TODO_ITEM_CREATED", "{}", (now - Duration::minutes(20)).format("%Y-%m-%d %H:%M:%S").to_string(), (now - Duration::minutes(20)).format("%Y-%m-%d %H:%M:%S").to_string()),
        ).await.expect("insert e1 failed");

        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), entity1.to_string(), "TODO_ITEM_CREATED", "{}", (now - Duration::hours(2)).format("%Y-%m-%d %H:%M:%S").to_string(), (now - Duration::hours(2)).format("%Y-%m-%d %H:%M:%S").to_string()),
        ).await.expect("insert e2 failed");

        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), entity2.to_string(), "TODO_ITEM_CREATED", "{}", (now - Duration::minutes(10)).format("%Y-%m-%d %H:%M:%S").to_string(), (now - Duration::minutes(10)).format("%Y-%m-%d %H:%M:%S").to_string()),
        ).await.expect("insert e3 failed");

        tx.commit().await.expect("commit failed");

        let filter = EventFilter::new().entity_id(entity1).start(start).end(end);
        let events = find(&conn, &filter).await.expect("find failed");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].entity_id, entity1);
    }

    #[tokio::test]
    async fn list_by_date_range_returns_events_in_range() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let now = Utc::now();
        let start = now - Duration::hours(1);
        let end = now + Duration::hours(1);

        let entity_id = Uuid::now_v7();
        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), entity_id.to_string(), "TODO_ITEM_CREATED", "{}", (start + Duration::minutes(10)).format("%Y-%m-%d %H:%M:%S").to_string(), (start + Duration::minutes(10)).format("%Y-%m-%d %H:%M:%S").to_string()),
        ).await.expect("insert e1 failed");

        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), entity_id.to_string(), "TODO_ITEM_STATUS_CHANGED", "{}", (start + Duration::minutes(30)).format("%Y-%m-%d %H:%M:%S").to_string(), (start + Duration::minutes(30)).format("%Y-%m-%d %H:%M:%S").to_string()),
        ).await.expect("insert e2 failed");

        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), entity_id.to_string(), "MEETING_CREATED", "{}", (start - Duration::hours(2)).format("%Y-%m-%d %H:%M:%S").to_string(), (start - Duration::hours(2)).format("%Y-%m-%d %H:%M:%S").to_string()),
        ).await.expect("insert e3 failed");

        tx.commit().await.expect("commit failed");

        let events = list_by_date_range(&conn, start, end)
            .await
            .expect("list failed");

        assert_eq!(events.len(), 2);
    }

    #[tokio::test]
    async fn list_by_date_range_orders_oldest_first() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let now = Utc::now();
        let entity_id = Uuid::now_v7();

        let t1 = now - Duration::minutes(30);
        let t2 = now - Duration::minutes(20);
        let t3 = now - Duration::minutes(10);

        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), entity_id.to_string(), "TODO_ITEM_CREATED", "{}", t3.format("%Y-%m-%d %H:%M:%S").to_string(), t3.format("%Y-%m-%d %H:%M:%S").to_string()),
        ).await.expect("insert e1 failed");

        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), entity_id.to_string(), "TODO_ITEM_CREATED", "{}", t1.format("%Y-%m-%d %H:%M:%S").to_string(), t1.format("%Y-%m-%d %H:%M:%S").to_string()),
        ).await.expect("insert e2 failed");

        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), entity_id.to_string(), "TODO_ITEM_CREATED", "{}", t2.format("%Y-%m-%d %H:%M:%S").to_string(), t2.format("%Y-%m-%d %H:%M:%S").to_string()),
        ).await.expect("insert e3 failed");

        tx.commit().await.expect("commit failed");

        let start = now - Duration::hours(1);
        let end = now + Duration::hours(1);
        let events = list_by_date_range(&conn, start, end)
            .await
            .expect("list failed");

        assert_eq!(events.len(), 3);
        assert_eq!(
            events[0].created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            t1.format("%Y-%m-%d %H:%M:%S").to_string()
        );
        assert_eq!(
            events[1].created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            t2.format("%Y-%m-%d %H:%M:%S").to_string()
        );
        assert_eq!(
            events[2].created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            t3.format("%Y-%m-%d %H:%M:%S").to_string()
        );
    }

    #[tokio::test]
    async fn list_by_date_range_empty() {
        let conn = setup().await;

        let start = Utc::now() - Duration::hours(1);
        let end = Utc::now() + Duration::hours(1);

        let events = list_by_date_range(&conn, start, end)
            .await
            .expect("list failed");

        assert_eq!(events.len(), 0);
    }

    #[tokio::test]
    async fn delete_by_entity_id_removes_events() {
        let mut conn = setup().await;
        let entity_id = Uuid::now_v7();

        let tx = conn.transaction().await.expect("tx begin failed");
        insert(&tx, entity_id, &EventType::TodoItemCreated, "{}")
            .await
            .expect("insert failed");
        delete_by_entity_id(&tx, entity_id)
            .await
            .expect("delete failed");
        tx.commit().await.expect("commit failed");

        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM event WHERE entity_id = ?",
                [entity_id.to_string()],
            )
            .await
            .expect("query failed");

        let row = rows
            .next()
            .await
            .expect("row fetch failed")
            .expect("count row not found");

        let count = *row
            .get_value(0)
            .expect("value extraction failed")
            .as_integer()
            .expect("expected integer");
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn delete_by_entity_id_noop_for_nonexistent() {
        let mut conn = setup().await;
        let nonexistent_id = Uuid::now_v7();

        let tx = conn.transaction().await.expect("tx begin failed");
        delete_by_entity_id(&tx, nonexistent_id)
            .await
            .expect("delete should not fail for nonexistent entity");
        tx.commit().await.expect("commit failed");
    }
}
