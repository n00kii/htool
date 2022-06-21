use std::{sync::Arc, num::IntErrorKind};
use super::Config;
use image_hasher::{HasherConfig, HashAlg};
use rusqlite::{Connection, Result as SqlResult, params};
use anyhow::{Context, Error, Result};
use infer;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Data {
    config: Arc<Config>,
    hasher_config: Arc<HasherConfig>
}

pub enum ImportationResult {
    Success,
    Duplicate,
    Fail(anyhow::Error),
}

impl Data {
    pub fn load_media_bytes(config: Arc<Config>, hash: String) {
        
    }
    pub fn register_media(config: Arc<Config>, bytes: &[u8], filekind: Option<infer::Type>) -> Result<ImportationResult> {
        println!("got {} kB for register", bytes.len() / 1000);
        let hasher_config = HasherConfig::new().hash_alg(HashAlg::DoubleGradient);
        let hasher = hasher_config.to_hasher();
        let db_path = config.path.database()?;
        let conn = Connection::open(&db_path)?;
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS media_info (
                hash TEXT PRIMARY KEY NOT NULL,
                p_hash TEXT,
                mime TEXT,
                link_id TEXT,
                date_registered INTEGER,
                size INTEGER,
                tags JSON
            )",
            [],
        )?;
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS media_bytes (
                hash TEXT PRIMARY KEY NOT NULL,
                bytes BLOB
            )",
            [],
        )?;
        
        let sha_hash = sha256::digest_bytes(bytes);
        let mut p_hash = None;
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let mime_type = match filekind {
            Some(kind) => {
                kind.mime_type()
            }
            None => {
                ""
            }
        };

        if let Some(filekind) = filekind {
            if filekind.matcher_type() == infer::MatcherType::Image {
                let image = image::load_from_memory(bytes)?;
                p_hash = Some(hex::encode(hasher.hash_image(&image).as_bytes()));
            }
        }
        
        conn.execute(
            "INSERT INTO media_bytes (hash, bytes) VALUES (?1, ?2)",
            params![sha_hash, bytes],
        )?;

        if p_hash.is_some() {
            conn.execute(
                "INSERT INTO media_info (hash, p_hash, mime, date_registered, size) 
                VALUES (?1, ?2, ?3, ?4, ?5)",
                params![sha_hash, p_hash.unwrap(), mime_type, timestamp, bytes.len()],
            )?;
        } else  {
            conn.execute(
                "INSERT INTO media_info (hash, mime, date_registered, size) 
                VALUES (?1, ?2, ?3, ?4)",
                params![sha_hash, mime_type, timestamp, bytes.len()],
            )?;
        }
        
        println!("success");
        Ok(ImportationResult::Success)
    }
}