use super::*;

#[test]
fn source_catalog_migration_is_repeatable_and_preserves_existing_data() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!("../migrations/0001_init.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../migrations/0002_sessions.sql"))
        .unwrap();
    conn.execute("INSERT INTO topics(slug,label_vi,label_en,enabled) VALUES ('legacy-topic','Chủ đề cũ','Old topic',0)", []).unwrap();
    conn.execute("INSERT INTO sources(slug,name,url,tier,enabled) VALUES ('legacy-source','Old source','https://example.invalid/feed','T2',0)", []).unwrap();
    let db = Database::initialize(conn).unwrap();
    let original: (String, i64, i64) = db
        .conn
        .query_row(
            "SELECT name,enabled,fetch_interval_minutes FROM sources WHERE slug='legacy-source'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(original, ("Old source".into(), 0, 60));
    let db = Database::initialize(db.conn).unwrap();
    let migration_count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version=3",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let enabled: i64 = db
        .conn
        .query_row(
            "SELECT enabled FROM topics WHERE slug='legacy-topic'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(migration_count, 1);
    assert_eq!(enabled, 0);
}
