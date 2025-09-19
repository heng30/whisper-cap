pub mod def;

#[allow(unused)]
pub use sqldb::{create_db, entry};

pub async fn init(db_path: &str) {
    create_db(db_path).await.expect("create db");

    entry::new(def::TRANSCRIBE_TABLE)
        .await
        .expect("transcribe table failed");

    entry::new(def::MODEL_TABLE)
        .await
        .expect("model table failed");
}
