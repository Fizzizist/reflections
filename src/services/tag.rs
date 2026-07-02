use anyhow::Result;
use regex::Regex;
use turso::Connection;
use turso::transaction::Transaction;
use uuid::Uuid;

use crate::repositories;

#[derive(Clone)]
pub struct TagService {
    #[allow(dead_code)]
    conn: Connection,
}

impl TagService {
    #[allow(dead_code)]
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    pub fn extract_tags(content: &str) -> Vec<String> {
        let re = Regex::new(r"(^|[\s(])(#[A-Za-z][A-Za-z0-9_-]+)").expect("regex compile failed");
        let mut seen = std::collections::HashSet::new();
        let mut labels = Vec::new();

        for caps in re.captures_iter(content) {
            if let Some(tag_match) = caps.get(2) {
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
        for label in labels {
            let tag_id = Uuid::now_v7();
            let tag = repositories::tag::insert(tx, tag_id, label).await?;

            let entity_tag_id = Uuid::now_v7();
            repositories::tag::insert_entity_tag(tx, entity_tag_id, tag.id, entity_id).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;

    async fn setup() -> TagService {
        let db = turso::Builder::new_local(":memory:")
            .experimental_custom_types(true)
            .build()
            .await
            .expect("db build failed");
        let conn = db.connect().expect("db connect failed");
        schema::init_schema(&conn)
            .await
            .expect("schema init failed");
        TagService::new(conn)
    }

    #[test]
    fn extract_tags_finds_hash_tags() {
        let content = "#alpha #beta-project #gamma123";
        let tags = TagService::extract_tags(content);
        assert_eq!(tags, vec!["alpha", "beta-project", "gamma123"]);
    }

    #[test]
    fn extract_tags_ignores_headings() {
        let content = "# Heading\n## Sub";
        let tags = TagService::extract_tags(content);
        assert_eq!(tags, Vec::<String>::new());
    }

    #[test]
    fn extract_tags_ignores_double_hash_no_space() {
        let content = "##double";
        let tags = TagService::extract_tags(content);
        assert_eq!(tags, Vec::<String>::new());
    }

    #[test]
    fn extract_tags_deduplicates() {
        let content = "#alpha #alpha #alpha";
        let tags = TagService::extract_tags(content);
        assert_eq!(tags, vec!["alpha"]);
    }

    #[test]
    fn extract_tags_case_insensitive() {
        let content = "#Alpha #ALPHA";
        let tags = TagService::extract_tags(content);
        assert_eq!(tags, vec!["alpha"]);
    }

    #[test]
    fn extract_tags_with_punctuation_prefix() {
        let content = "(#alpha) and #beta";
        let tags = TagService::extract_tags(content);
        assert_eq!(tags, vec!["alpha", "beta"]);
    }

    #[tokio::test]
    async fn sync_tags_creates_tags_and_links() {
        let mut svc = setup().await;
        let entity_id = Uuid::now_v7();
        let labels = vec!["alpha".to_string(), "beta".to_string()];

        let tx = svc.conn.transaction().await.expect("tx begin failed");
        TagService::sync_tags(&tx, entity_id, &labels)
            .await
            .expect("sync_tags failed");
        tx.commit().await.expect("commit failed");

        let alpha_tag = repositories::tag::find_by_label(&svc.conn, "alpha")
            .await
            .expect("find_by_label failed")
            .expect("alpha tag should exist");

        let beta_tag = repositories::tag::find_by_label(&svc.conn, "beta")
            .await
            .expect("find_by_label failed")
            .expect("beta tag should exist");

        let entity_tags = repositories::tag::find_tags_for_entity(&svc.conn, entity_id)
            .await
            .expect("find_tags_for_entity failed");

        assert_eq!(entity_tags.len(), 2);
        assert!(entity_tags.iter().any(|t| t.id == alpha_tag.id));
        assert!(entity_tags.iter().any(|t| t.id == beta_tag.id));
    }

    #[tokio::test]
    async fn sync_tags_upserts_existing_tags() {
        let mut svc = setup().await;
        let entity_id1 = Uuid::now_v7();
        let entity_id2 = Uuid::now_v7();

        let tx1 = svc.conn.transaction().await.expect("tx1 begin failed");
        TagService::sync_tags(&tx1, entity_id1, &vec!["alpha".to_string()])
            .await
            .expect("sync_tags failed");
        tx1.commit().await.expect("commit1 failed");

        let tx2 = svc.conn.transaction().await.expect("tx2 begin failed");
        TagService::sync_tags(&tx2, entity_id2, &vec!["alpha".to_string()])
            .await
            .expect("sync_tags failed");
        tx2.commit().await.expect("commit2 failed");

        let alpha_tag = repositories::tag::find_by_label(&svc.conn, "alpha")
            .await
            .expect("find_by_label failed")
            .expect("alpha tag should exist");

        assert_eq!(alpha_tag.label, "alpha");

        let tags_for_entity1 = repositories::tag::find_tags_for_entity(&svc.conn, entity_id1)
            .await
            .expect("find_tags_for_entity failed");
        let tags_for_entity2 = repositories::tag::find_tags_for_entity(&svc.conn, entity_id2)
            .await
            .expect("find_tags_for_entity failed");

        assert_eq!(tags_for_entity1.len(), 1);
        assert_eq!(tags_for_entity2.len(), 1);
        assert_eq!(tags_for_entity1[0].id, tags_for_entity2[0].id);
    }
}
