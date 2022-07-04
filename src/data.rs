use super::Config;
use anyhow::{Context, Error, Result};
use image_hasher::{HashAlg, HasherConfig};
use infer;
use rusqlite::{named_params, params, Connection, Result as SqlResult};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{num::IntErrorKind, path::PathBuf, sync::Arc};

pub struct Data {
    config: Arc<Config>,
    hasher_config: Arc<HasherConfig>,
}
#[derive(Debug)]
pub enum ImportationResult {
    Success,
    Duplicate,
    Fail(anyhow::Error),
}

impl PartialEq for ImportationResult {
    fn eq(&self, other: &Self) -> bool {
        use ImportationResult::*;
        match (self, other) {
            (Success, Success) => true,
            (Duplicate, Duplicate) => true,
            (Fail(e1), Fail(e2)) => true,
            _ => false,
        }
    }
}

impl Data {
    pub fn initialize_database_connection(db_path: &PathBuf) -> Result<Connection> {
        let conn = Connection::open(&db_path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS media_info (
                hash TEXT PRIMARY KEY NOT NULL,
                perceptual_hash TEXT,
                mime TEXT,
                date_registered INTEGER,
                size INTEGER
            )", // and more meta tags
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS media_bytes (
                hash TEXT PRIMARY KEY NOT NULL,
                bytes BLOB
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS tag_info (
                tag TEXT PRIMARY KEY NOT NULL,
                description TEXT
            )",
            [],
        )?;

        conn.execute(
            // vertical array
            "CREATE TABLE IF NOT EXISTS media_tags (
                hash TEXT NOT NULL,
                tag TEXT
            )",
            [],
        )?;

        conn.execute(
            // vertical array
            "CREATE TABLE IF NOT EXISTS media_links (
                id INTEGER NOT NULL,
                value INTEGER,
                type TEXT,
                hash TEXT
            )",
            [],
        )?;
        Ok(conn)
    }
    pub fn load_media_bytes(config: Arc<Config>, hash: String) {}
    pub fn register_media(config: Arc<Config>, bytes: &[u8], filekind: Option<infer::Type>, linking_dir: Option<String>, dir_link_map: Arc<Mutex<HashMap<String, i32>>>) -> ImportationResult {
        fn register(config: Arc<Config>, bytes: &[u8], filekind: Option<infer::Type>, linking_dir: Option<String>, dir_link_map: Arc<Mutex<HashMap<String, i32>>>) -> Result<ImportationResult> {
            println!("got {} kB for register", bytes.len() / 1000);
            let hasher_config = HasherConfig::new().hash_alg(HashAlg::DoubleGradient);
            let hasher = hasher_config.to_hasher();
            let db_path = config.path.database()?;
            let conn = Data::initialize_database_connection(&db_path)?;

            let sha_hash = sha256::digest_bytes(bytes);
            let mut perceptual_hash: Option<String> = None;
            let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
            let mime_type = match filekind {
                Some(kind) => kind.mime_type(),
                None => "",
            };

            
            if let Some(filekind) = filekind {
                if filekind.matcher_type() == infer::MatcherType::Image {
                    let image = image::load_from_memory(bytes)?;
                    perceptual_hash = Some(hex::encode(hasher.hash_image(&image).as_bytes()));
                }
            }
            
            let mut statement = conn.prepare("SELECT 1 FROM media_info WHERE hash = ?")?;
            let exists = statement.exists(params![sha_hash])?;
            if exists { return Ok(ImportationResult::Duplicate) }

            
            let insert_result = if perceptual_hash.is_some() {
                conn.execute(
                    "INSERT INTO media_info (hash, perceptual_hash, mime, date_registered, size)
                    VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![sha_hash, perceptual_hash.unwrap(), mime_type, timestamp, bytes.len()],
                )
            } else {
                conn.execute(
                    "INSERT INTO media_info (hash, mime, date_registered, size)
                    VALUES (?1, ?2, ?3, ?4)",
                    params![sha_hash, mime_type, timestamp, bytes.len()],
                )
            };

            match insert_result {
                Ok(_) => {
                    conn.execute("INSERT INTO media_bytes (hash, bytes) VALUES (?1, ?2)", params![sha_hash, bytes])?;
                    // conn.execute(sql, params);
                    if let Some(linking_dir) = linking_dir {
                        if let Ok(mut dir_link_map) = dir_link_map.lock() {
                            if let Some(link_id) = dir_link_map.get(&linking_dir) {
                                conn.execute(
                                    "INSERT INTO media_links (id, hash)
                                    VALUES (?1, ?2)",
                                    params![link_id, sha_hash],
                                )?;
                            } else { // new link_id
                                let next_id: i32 = conn.query_row("SELECT IFNULL(MAX(id), 0) + 1 FROM media_links ", [], |row| {
                                    row.get(0)
                                })?;
                                conn.execute(
                                    "INSERT INTO media_links (id, hash)
                                    VALUES (?1, ?2)",
                                    params![next_id, sha_hash])?;

                                dir_link_map.insert(linking_dir, next_id);
                            }
                        }
                    }
                    
                    return Ok(ImportationResult::Success);
                },
                Err(error) => {
                    // if (let rusqlite::Error::SqliteFailure(e, _) = error) && e.code == rusqlite::ErrorCode::ConstraintViolation { waiting for rust 1.62 :(
                    if let rusqlite::Error::SqliteFailure(e, _) = error { 
                        if e.code == rusqlite::ErrorCode::ConstraintViolation {
                            return Ok(ImportationResult::Duplicate)
                        }
                    }
                    return Ok(ImportationResult::Fail(error.into()))
                }
            }
            
        }

        match register(config, bytes, filekind, linking_dir, dir_link_map) {
            Ok(ImportationResult::Success) => return ImportationResult::Success,
            Ok(ImportationResult::Duplicate) => return ImportationResult::Duplicate,
            Ok(ImportationResult::Fail(error)) => return ImportationResult::Fail(error),
            Err(error) => return ImportationResult::Fail(error),
        };
    }
}
