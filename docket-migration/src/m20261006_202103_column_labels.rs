//! The group, tags and theme columns copied to labels, for the items written since labels became
//! rows: a group becomes the label `group:NAME`, each tag but a priority word becomes a label, and a
//! theme that names neither a release nor an area of its project becomes a label of that name. An
//! item keeps every label it already carries, and the columns are left for the drop that follows.

use sea_orm_migration::prelude::*;

/// Each label an item's columns give it: `(rid, project, name)`.
const GIVEN: &str = r"
  SELECT i.rid, i.project, 'group:' || btrim(i.group_name) AS name FROM items i
   WHERE btrim(coalesce(i.group_name, '')) <> ''
  UNION SELECT i.rid, i.project, btrim(t.tag) FROM items i, jsonb_array_elements_text(i.tags) t(tag)
   WHERE btrim(t.tag) <> '' AND lower(btrim(t.tag)) NOT IN ('critical', 'high', 'normal', 'low')
  UNION SELECT i.rid, i.project, btrim(i.theme) FROM items i
   WHERE btrim(coalesce(i.theme, '')) <> ''
     AND NOT EXISTS (SELECT 1 FROM releases r
                      WHERE r.project = i.project AND lower(r.name) = lower(btrim(i.theme)))
     AND NOT EXISTS (SELECT 1 FROM areas a
                      WHERE a.project = i.project AND lower(a.name) = lower(btrim(i.theme)))";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let c = manager.get_connection();
        c.execute_unprepared(&format!(
            "INSERT INTO labels (project, name) SELECT DISTINCT g.project, g.name FROM ({GIVEN}) g \
             ON CONFLICT (project, lower(name)) DO NOTHING"
        ))
        .await?;
        c.execute_unprepared(&format!(
            "INSERT INTO item_labels (rid, label_id) SELECT g.rid, l.id FROM ({GIVEN}) g \
             JOIN labels l ON l.project = g.project AND lower(l.name) = lower(g.name) \
             ON CONFLICT DO NOTHING"
        ))
        .await?;
        Ok(())
    }

    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
