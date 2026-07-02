use anyhow::Context;
use anyhow::Result;
use regex::Regex;
use std::path::Path;
use tokio::fs;
use turso::Connection;
use turso::transaction::Transaction;
use uuid::Uuid;

use crate::repositories;

static TAG_REGEX: &str = r"(?:^|[\s(.,;:!?\[\]/])(#[A-Za-z][A-Za-z0-9_-]+)";

pub fn extract_tags(content: &str) -> Vec<String> {
    let re = Regex::new(TAG_REGEX)
        .expect("tag extraction regex is a static literal, cannot fail at runtime");
    let mut seen = std::collections::HashSet::new();
    let mut labels = Vec::new();

    for caps in re.captures_iter(content) {
        if let Some(tag_match) = caps.get(1) {
            let tag_with_hash = tag_match.as_str();
            let label = tag_with_hash[1..].to_lowercase();

            if seen.insert(label.clone()) {
                labels.push(label);
            }
        }
    }

    labels
}

pub async fn sync_tags(tx: &Transaction<'_>, entity_id: Uuid, labels: &[String]) -> Result<()> {
    if labels.is_empty() {
        return Ok(());
    }

    let entries: Vec<(Uuid, String)> = labels
        .iter()
        .map(|label| (Uuid::now_v7(), label.clone()))
        .collect();

    let tag_ids = repositories::tag::insert_batch(tx, &entries)
        .await
        .context("batch upserting tags")?;

    repositories::tag::insert_entity_tag_batch(tx, &tag_ids, entity_id)
        .await
        .context("batch linking tags to entity")?;

    Ok(())
}

/// Extracts tags from file content and links them to the entity.
/// Called once during cleanup when content is confirmed non-empty.
/// Tags accumulate — stale tag removal is not implemented because
/// cleanup only fires once per entity lifecycle.
pub async fn sync_tags_from_file(
    conn: &mut Connection,
    entity_id: Uuid,
    file_path: &Path,
) -> Result<()> {
    let content = fs::read_to_string(file_path).await?;
    let labels = extract_tags(&content);
    if labels.is_empty() {
        return Ok(());
    }
    let tx = conn.transaction().await?;
    sync_tags(&tx, entity_id, &labels).await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;
    use turso::Connection;

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

    #[test]
    fn extract_tags_finds_hash_tags() {
        let content = "#alpha #beta-project #gamma123";
        let tags = extract_tags(content);
        assert_eq!(tags, vec!["alpha", "beta-project", "gamma123"]);
    }

    #[test]
    fn extract_tags_ignores_headings() {
        let content = "# Heading\n## Sub";
        let tags = extract_tags(content);
        assert_eq!(tags, Vec::<String>::new());
    }

    #[test]
    fn extract_tags_ignores_double_hash_no_space() {
        let content = "##double";
        let tags = extract_tags(content);
        assert_eq!(tags, Vec::<String>::new());
    }

    #[test]
    fn extract_tags_deduplicates() {
        let content = "#alpha #alpha #alpha";
        let tags = extract_tags(content);
        assert_eq!(tags, vec!["alpha"]);
    }

    #[test]
    fn extract_tags_case_insensitive() {
        let content = "#Alpha #ALPHA";
        let tags = extract_tags(content);
        assert_eq!(tags, vec!["alpha"]);
    }

    #[test]
    fn extract_tags_with_punctuation_prefix() {
        let content = "(#alpha) and #beta";
        let tags = extract_tags(content);
        assert_eq!(tags, vec!["alpha", "beta"]);
    }

    #[test]
    fn extract_tags_with_comma_prefix() {
        let content = "work on #alpha, then #beta";
        let tags = extract_tags(content);
        assert_eq!(tags, vec!["alpha", "beta"]);
    }

    #[test]
    fn extract_tags_with_newline_prefix() {
        let content = "line1\n#alpha\nline3 #beta";
        let tags = extract_tags(content);
        assert_eq!(tags, vec!["alpha", "beta"]);
    }

    #[test]
    fn extract_tags_no_tags_returns_empty() {
        let content = "just some regular text without tags";
        let tags = extract_tags(content);
        assert_eq!(tags, Vec::<String>::new());
    }

    #[tokio::test]
    async fn sync_tags_creates_tags_and_links() {
        let mut conn = setup().await;
        let entity_id = Uuid::now_v7();
        let labels = vec!["alpha".to_string(), "beta".to_string()];

        let tx = conn.transaction().await.expect("tx begin failed");
        sync_tags(&tx, entity_id, &labels)
            .await
            .expect("sync_tags failed");
        tx.commit().await.expect("commit failed");

        let alpha_tag = repositories::tag::find_by_label(&conn, "alpha")
            .await
            .expect("find_by_label failed")
            .expect("alpha tag should exist");

        let beta_tag = repositories::tag::find_by_label(&conn, "beta")
            .await
            .expect("find_by_label failed")
            .expect("beta tag should exist");

        let entity_tags = repositories::tag::find_tags_for_entity(&conn, entity_id)
            .await
            .expect("find_tags_for_entity failed");

        assert_eq!(entity_tags.len(), 2);
        assert!(entity_tags.iter().any(|t| t.id == alpha_tag.id));
        assert!(entity_tags.iter().any(|t| t.id == beta_tag.id));
    }

    #[tokio::test]
    async fn sync_tags_upserts_existing_tags() {
        let mut conn = setup().await;
        let entity_id1 = Uuid::now_v7();
        let entity_id2 = Uuid::now_v7();

        let tx1 = conn.transaction().await.expect("tx1 begin failed");
        sync_tags(&tx1, entity_id1, &vec!["alpha".to_string()])
            .await
            .expect("sync_tags failed");
        tx1.commit().await.expect("commit1 failed");

        let tx2 = conn.transaction().await.expect("tx2 begin failed");
        sync_tags(&tx2, entity_id2, &vec!["alpha".to_string()])
            .await
            .expect("sync_tags failed");
        tx2.commit().await.expect("commit2 failed");

        let alpha_tag = repositories::tag::find_by_label(&conn, "alpha")
            .await
            .expect("find_by_label failed")
            .expect("alpha tag should exist");

        assert_eq!(alpha_tag.label, "alpha");

        let tags_for_entity1 = repositories::tag::find_tags_for_entity(&conn, entity_id1)
            .await
            .expect("find_tags_for_entity failed");
        let tags_for_entity2 = repositories::tag::find_tags_for_entity(&conn, entity_id2)
            .await
            .expect("find_tags_for_entity failed");

        assert_eq!(tags_for_entity1.len(), 1);
        assert_eq!(tags_for_entity2.len(), 1);
        assert_eq!(tags_for_entity1[0].id, tags_for_entity2[0].id);
    }

    #[tokio::test]
    async fn sync_tags_failure_rolls_back() {
        let mut conn = setup().await;
        let entity_id = Uuid::now_v7();

        let tx = conn.transaction().await.expect("tx begin failed");
        sync_tags(&tx, entity_id, &["alpha".to_string(), "beta".to_string()])
            .await
            .expect("sync_tags should succeed");
        drop(tx);

        let tags = repositories::tag::find_tags_for_entity(&conn, entity_id)
            .await
            .expect("find_tags_for_entity failed");
        assert_eq!(
            tags.len(),
            0,
            "tags should not persist when transaction is dropped without commit"
        );

        let alpha = repositories::tag::find_by_label(&conn, "alpha")
            .await
            .expect("find_by_label failed");
        assert!(alpha.is_none(), "tag row should not persist without commit");
    }
}
