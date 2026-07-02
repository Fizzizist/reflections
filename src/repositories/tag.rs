use anyhow::Result;
use chrono::Utc;
use turso::{Connection, Error::QueryReturnedNoRows, transaction::Transaction};
use uuid::Uuid;

use crate::models::tag::Tag;
use crate::repositories::parse_timestamp;

fn row_to_tag(row: &turso::Row) -> Result<Tag> {
    let id_str: String = row.get(0)?;
    let id = Uuid::parse_str(&id_str)?;
    let label: String = row.get(1)?;
    let created_str: String = row.get(2)?;
    let created_at = parse_timestamp(&created_str)?;
    let updated_str: String = row.get(3)?;
    let updated_at = parse_timestamp(&updated_str)?;

    Ok(Tag {
        id,
        label,
        created_at,
        updated_at,
    })
}

pub async fn insert(tx: &Transaction<'_>, id: Uuid, label: &str) -> Result<Tag> {
    let sql = r#"INSERT INTO tag (tag_id, label, created_at, updated_at) VALUES (?, ?, ?, ?)
                 ON CONFLICT(label) DO UPDATE SET updated_at = excluded.updated_at
                 RETURNING tag_id, label, created_at, updated_at"#;
    let now = Utc::now().to_rfc3339();
    let mut rows = tx
        .query(sql, (id.to_string(), label.to_string(), now.clone(), now))
        .await?;
    let row = rows.next().await?;
    while rows.next().await?.is_some() {}
    if let Some(row) = row {
        return row_to_tag(&row);
    }
    Err(QueryReturnedNoRows.into())
}

#[allow(dead_code)]
pub async fn find_by_label(conn: &Connection, label: &str) -> Result<Option<Tag>> {
    let sql = "SELECT tag_id, label, created_at, updated_at FROM tag WHERE label = ?";
    let mut rows = conn.query(sql, (label.to_string(),)).await?;
    let row = rows.next().await?;
    while rows.next().await.is_ok_and(|r| r.is_some()) {}
    if let Some(row) = row {
        return Ok(Some(row_to_tag(&row)?));
    }
    Ok(None)
}

pub async fn insert_entity_tag(
    tx: &Transaction<'_>,
    entity_tag_id: Uuid,
    tag_id: Uuid,
    entity_id: Uuid,
) -> Result<()> {
    let sql = r#"INSERT OR IGNORE INTO entity_tag (entity_tag_id, tag_id, entity_id, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?)"#;
    let now = Utc::now().to_rfc3339();
    tx.execute(
        sql,
        (
            entity_tag_id.to_string(),
            tag_id.to_string(),
            entity_id.to_string(),
            now.clone(),
            now,
        ),
    )
    .await?;
    Ok(())
}

#[allow(dead_code)]
pub async fn find_tags_for_entity(conn: &Connection, entity_id: Uuid) -> Result<Vec<Tag>> {
    let sql = r#"SELECT t.tag_id, t.label, t.created_at, t.updated_at
                 FROM tag t
                 JOIN entity_tag et ON t.tag_id = et.tag_id
                 WHERE et.entity_id = ?"#;
    let mut rows = conn.query(sql, (entity_id.to_string(),)).await?;

    let mut tags = Vec::new();
    while let Some(row) = rows.next().await? {
        tags.push(row_to_tag(&row)?);
    }

    Ok(tags)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;

    async fn setup() -> Connection {
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
    async fn insert_tag_and_retrieve() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let id = Uuid::now_v7();
        let _tag = insert(&tx, id, "test-label").await.expect("insert failed");
        tx.commit().await.expect("commit failed");

        let found = find_by_label(&conn, "test-label")
            .await
            .expect("find_by_label failed")
            .expect("expected tag to exist");

        assert_eq!(found.label, "test-label");
    }

    #[tokio::test]
    async fn insert_duplicate_label_upserts() {
        let mut conn = setup().await;

        let id1 = Uuid::now_v7();
        let tx1 = conn.transaction().await.expect("tx1 begin failed");
        insert(&tx1, id1, "duplicate-label")
            .await
            .expect("first insert failed");
        tx1.commit().await.expect("commit1 failed");

        let id2 = Uuid::now_v7();
        let tx2 = conn.transaction().await.expect("tx2 begin failed");
        let tag2 = insert(&tx2, id2, "duplicate-label")
            .await
            .expect("second insert failed");
        tx2.commit().await.expect("commit2 failed");

        let found = find_by_label(&conn, "duplicate-label")
            .await
            .expect("find_by_label failed");

        let found = found.expect("expected tag to exist");
        assert_eq!(found.label, "duplicate-label");
        assert_eq!(found.id, tag2.id);
    }

    #[tokio::test]
    async fn insert_entity_tag_link() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let tag_id = Uuid::now_v7();
        let _tag = insert(&tx, tag_id, "entity-tag")
            .await
            .expect("insert tag failed");

        let entity_id = Uuid::now_v7();
        let entity_tag_id = Uuid::now_v7();
        insert_entity_tag(&tx, entity_tag_id, _tag.id, entity_id)
            .await
            .expect("insert_entity_tag failed");
        tx.commit().await.expect("commit failed");

        let tags = find_tags_for_entity(&conn, entity_id)
            .await
            .expect("find_tags_for_entity failed");

        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].id, _tag.id);
        assert_eq!(tags[0].label, "entity-tag");
    }

    #[tokio::test]
    async fn duplicate_entity_tag_link_is_idempotent() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let tag_id = Uuid::now_v7();
        let _tag = insert(&tx, tag_id, "idempotent-tag")
            .await
            .expect("insert tag failed");

        let entity_id = Uuid::now_v7();
        let entity_tag_id1 = Uuid::now_v7();
        let entity_tag_id2 = Uuid::now_v7();

        insert_entity_tag(&tx, entity_tag_id1, _tag.id, entity_id)
            .await
            .expect("first insert_entity_tag failed");

        insert_entity_tag(&tx, entity_tag_id2, _tag.id, entity_id)
            .await
            .expect("second insert_entity_tag failed");

        tx.commit().await.expect("commit failed");

        let tags = find_tags_for_entity(&conn, entity_id)
            .await
            .expect("find_tags_for_entity failed");

        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].id, _tag.id);
    }
}
