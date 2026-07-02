use anyhow::Result;
use chrono::Utc;
#[cfg(test)]
use turso::Connection;
use turso::params_from_iter;
use turso::transaction::Transaction;
use uuid::Uuid;

#[cfg(test)]
use crate::models::tag::Tag;
#[cfg(test)]
use crate::repositories::parse_timestamp;

pub async fn insert_batch(tx: &Transaction<'_>, entries: &[(Uuid, String)]) -> Result<Vec<Uuid>> {
    if entries.is_empty() {
        return Ok(Vec::new());
    }

    let now = Utc::now().to_rfc3339();
    let placeholders: Vec<&str> = (0..entries.len()).map(|_| "(?, ?, ?, ?)").collect();
    let sql = format!(
        r#"INSERT INTO tag (tag_id, label, created_at, updated_at) VALUES {}
           ON CONFLICT(label) DO UPDATE SET updated_at = excluded.updated_at
           RETURNING tag_id"#,
        placeholders.join(", ")
    );

    let mut params: Vec<String> = Vec::with_capacity(entries.len() * 4);
    for (id, label) in entries {
        params.push(id.to_string());
        params.push(label.clone());
        params.push(now.clone());
        params.push(now.clone());
    }

    let mut rows = tx.query(sql, params_from_iter(params)).await?;
    let mut tag_ids = Vec::new();
    while let Some(row) = rows.next().await? {
        let id_str: String = row.get(0)?;
        tag_ids.push(Uuid::parse_str(&id_str)?);
    }

    Ok(tag_ids)
}

pub async fn insert_entity_tag_batch(
    tx: &Transaction<'_>,
    tag_ids: &[Uuid],
    entity_id: Uuid,
) -> Result<()> {
    if tag_ids.is_empty() {
        return Ok(());
    }

    let now = Utc::now().to_rfc3339();
    let placeholders: Vec<&str> = (0..tag_ids.len()).map(|_| "(?, ?, ?, ?, ?)").collect();
    let sql = format!(
        r#"INSERT OR IGNORE INTO entity_tag (entity_tag_id, tag_id, entity_id, created_at, updated_at) VALUES {}"#,
        placeholders.join(", ")
    );

    let mut params: Vec<String> = Vec::with_capacity(tag_ids.len() * 5);
    for tag_id in tag_ids {
        params.push(Uuid::now_v7().to_string());
        params.push(tag_id.to_string());
        params.push(entity_id.to_string());
        params.push(now.clone());
        params.push(now.clone());
    }

    tx.execute(sql, params_from_iter(params)).await?;
    Ok(())
}

#[cfg(test)]
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

#[cfg(test)]
pub async fn find_by_label(conn: &Connection, label: &str) -> Result<Option<Tag>> {
    let sql = "SELECT tag_id, label, created_at, updated_at FROM tag WHERE label = ?";
    let mut rows = conn.query(sql, (label.to_string(),)).await?;
    let row = rows.next().await?;
    while rows.next().await?.is_some() {}
    if let Some(row) = row {
        return Ok(Some(row_to_tag(&row)?));
    }
    Ok(None)
}

#[cfg(test)]
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
