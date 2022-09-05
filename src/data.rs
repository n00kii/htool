use crate::tags::tags::Tag;
use crate::tags::tags::TagData;
use crate::tags::tags::TagLink;
use crate::tags::tags::TagLinkType;

use super::ui;
use super::Config;
use anyhow::{Context, Error, Result};
use egui_extras::RetainedImage;
use image::{imageops, EncodableLayout, FlatSamples, ImageBuffer, Rgba};
use image_hasher::{HashAlg, HasherConfig};
use infer;
use rusqlite::{named_params, params, Connection, Result as SqlResult};
use std::collections::HashMap;
use std::hash;
use std::io::Cursor;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{num::IntErrorKind, path::PathBuf, sync::Arc};

#[derive(Debug)]
pub enum ImportationStatus {
    PendingBytes,
    Success,
    Duplicate,
    Fail(anyhow::Error),
}

impl PartialEq for ImportationStatus {
    fn eq(&self, other: &Self) -> bool {
        use ImportationStatus::*;
        match (self, other) {
            (PendingBytes, PendingBytes) => true,
            (Success, Success) => true,
            (Duplicate, Duplicate) => true,
            (Fail(_e1), Fail(_e2)) => true,
            _ => false,
        }
    }
}

pub enum _EntryId {
    MediaEntry(String),
    MediaEntryPlural(i32),
}

pub struct MediaInfo {
    pub hash: String,
    pub mime: String,
    pub p_hash: Option<String>,
    pub date_registered: i64,
    pub size: i64,
    pub tags: Vec<Tag>,
}

pub struct MediaInfoPlural {
    hashes: Vec<String>,
    date_registered: i64,
    total_size: i64,
    tags: Vec<Tag>,
}

impl MediaInfo {
    pub fn includes_tags_or(&self) {}
    pub fn includes_tags_and(&self, tags: &Vec<Tag>) -> bool {
        for tag in tags {
            let includes_tag = self.tags.iter().any(|included_tag| included_tag == tag);
            if !includes_tag {
                return false
            }
        }
        true
    }
}

// todo: move stuff out of struct
pub fn generate_thumbnail(image_data: &[u8], thumbnail_size: u8) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>> {
    let image = image::load_from_memory(image_data)?;
    let (w, h) = (image.width(), image.height());
    let image_cropped = imageops::crop_imm(
        &image,
        if h > w { 0 } else { (w - h) / 2 },
        if w > h { 0 } else { (h - w) / 2 },
        if h > w { w } else { h },
        if w > h { h } else { w },
    )
    .to_image();
    let thumbnail = imageops::thumbnail(&image_cropped, thumbnail_size.into(), thumbnail_size.into());
    Ok(thumbnail)
}

// https://math.stackexchange.com/questions/4489146/filling-a-square-with-squares-along-the-diagonal
pub fn generate_plural_thumbnail(constituent_thumbnails: &Vec<ImageBuffer<Rgba<u8>, Vec<u8>>>) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>> {
    // https://www.desmos.com/calculator/moriiungym for tuning
    let thumbnail_size: u32 = 100;
    let distance_factor = 0.3; //1 = thumbnails half size away from each other, 0 = thumbnails on top of each other
    let size_factor = 0.3; // 0 = normal size; 1 = thumbnail_size size; >0.5 = constituent thumbnails clip out of bounds

    // size of the mini thumbnails

    let size_factor_scaled = 0.5 * (constituent_thumbnails.len() as f64) * size_factor + 1.0;
    let size_factor_positional_offset = thumbnail_size as f64 * distance_factor * (size_factor_scaled - 1.0) * 0.5;

    let constituent_thumbnail_size = ((2.0 * thumbnail_size as f64) / (constituent_thumbnails.len() as f64 + 1.0) * size_factor_scaled) as u32;

    let distance_factor_scaled: f64 = (1.0 - distance_factor) * (thumbnail_size - constituent_thumbnail_size) as f64 / 2.0;

    let position_offset = distance_factor_scaled - size_factor_positional_offset;

    let mut thumbnail = ImageBuffer::new(thumbnail_size, thumbnail_size);
    for (i, constituent_thumbnail) in constituent_thumbnails.iter().enumerate() {
        let resized_constituent_thumbnail = imageops::thumbnail(constituent_thumbnail, constituent_thumbnail_size, constituent_thumbnail_size);
        let position_x = (thumbnail_size as f64 - constituent_thumbnail_size as f64)
            - (i as f64 * (constituent_thumbnail_size / 2) as f64 * distance_factor + position_offset); // (thumbnail_size as i64 - constituent_thumbnail_size as i64) -
        let position_y = i as f64 * (constituent_thumbnail_size / 2) as f64 * distance_factor + position_offset;
        imageops::overlay(&mut thumbnail, &resized_constituent_thumbnail, position_x as i64, position_y as i64);
    }
    Ok(thumbnail)
}

//TODO consolidate below fxns using enum
pub fn load_thumbnail_plural(config: Arc<Config>, link_id: i32) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>> {
    let conn = initialize_database_connection(&config.path.database()?)?;
    let mut statement = conn.prepare("SELECT bytes FROM thumbnail_cache WHERE link_id = ?1")?;
    let bytes_res: Result<Vec<u8>, rusqlite::Error> = statement.query_row(params![link_id], |row| row.get(0));

    match bytes_res {
        Ok(bytes) => {
            let image = image::load_from_memory(&bytes)?;
            let image_buffer = image.into_rgba8();
            return Ok(image_buffer);
        }
        Err(error) => {
            if error == rusqlite::Error::QueryReturnedNoRows {
                let hashes_of_link = get_hashes_of_link(Arc::clone(&config), link_id)?;
                if hashes_of_link.len() == 0 {
                    return Err(anyhow::Error::msg("no hashes in link"));
                }
                let max_constituent_thumbnails = 3;
                let mut constituent_thumbnails = Vec::new();

                for hash in hashes_of_link {
                    if let Ok(constituent_thumbnail) = load_thumbnail(config.clone(), &hash) {
                        constituent_thumbnails.push(constituent_thumbnail);
                        if constituent_thumbnails.len() == max_constituent_thumbnails {
                            break;
                        }
                    }
                }
                if constituent_thumbnails.len() == 0 {
                    return Err(anyhow::Error::msg("couldnt load any thumbnails of hashes of link"));
                }

                let thumbnail = generate_plural_thumbnail(&constituent_thumbnails)?;
                let mut thumbnail_bytes: Vec<u8> = Vec::new();
                let mut writer = Cursor::new(&mut thumbnail_bytes);
                thumbnail.write_to(&mut writer, image::ImageOutputFormat::Png)?;

                conn.execute(
                    "INSERT INTO thumbnail_cache (link_id, bytes)
                        VALUES (?1, ?2)",
                    params![link_id, thumbnail_bytes],
                )?;

                return Ok(thumbnail); //todo use generate_thumbnail_plural
            }
            // todo!()
            return Err(error.into());
        }
    }
}

pub fn load_thumbnail(config: Arc<Config>, hash: &String) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>> {
    let conn = initialize_database_connection(&config.path.database()?)?;
    let mut statement = conn.prepare("SELECT bytes FROM thumbnail_cache WHERE hash = ?1")?;
    let bytes_res: Result<Vec<u8>, rusqlite::Error> = statement.query_row(params![hash], |row| row.get(0));

    match bytes_res {
        Ok(bytes) => {
            let image = image::load_from_memory(&bytes)?;
            let image_buffer = image.to_rgba8();
            Ok(image_buffer)
        }
        Err(error) => {
            if error == rusqlite::Error::QueryReturnedNoRows {
                let mut statement = conn.prepare("SELECT mime FROM media_info WHERE hash = ?1")?;
                let mime_type: String = statement.query_row(params![hash], |row| row.get("mime"))?;
                let bytes = load_bytes(Arc::clone(&config), hash)?;
                if mime_type.starts_with("image") {
                    let thumbnail = generate_thumbnail(&bytes, 100)?;
                    let mut thumbnail_bytes: Vec<u8> = Vec::new();
                    let mut writer = Cursor::new(&mut thumbnail_bytes);
                    thumbnail.write_to(&mut writer, image::ImageOutputFormat::Png)?;

                    conn.execute(
                        "INSERT INTO thumbnail_cache (hash, bytes)
                            VALUES (?1, ?2)",
                        params![hash, thumbnail_bytes],
                    )?;
                    return Ok(thumbnail);
                } else {
                    return Err(anyhow::Error::msg(format!("can't create thumbnail for {mime_type}")));
                }
                // need to generate thumbnail, cache it
            }
            // todo!()
            Err(error.into())
        }
    }
    // println!("b: {:?}", bytes);
}

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
                name TEXT,
                namespace TEXT,
                description TEXT
            )",
        [],
    )?;

    conn.execute(
        // vertical array
        "CREATE TABLE IF NOT EXISTS media_tags (
                hash TEXT,
                link_id INTEGER,
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

    conn.execute(
        "CREATE TABLE IF NOT EXISTS tag_links (
                type TEXT,
                from_tag TEXT,
                to_tag TEXT
            )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS thumbnail_cache (
                hash TEXT,
                link_id INTEGER,
                bytes BLOB
            )",
        [],
    )?;
    Ok(conn)
}
pub fn load_bytes(config: Arc<Config>, hash: &String) -> Result<Vec<u8>> {
    let conn = initialize_database_connection(&config.path.database()?)?;
    let mut statement = conn.prepare("SELECT bytes FROM media_bytes WHERE hash = ?1")?;
    let bytes: Vec<u8> = statement.query_row(params![hash], |row| row.get(0))?;
    Ok(bytes)
    // todo!()
}
pub fn load_media_info(config: Arc<Config>, hash: &String) -> Result<MediaInfo> {
    let conn = initialize_database_connection(&config.path.database()?)?;
    let mut statement = conn.prepare("SELECT * FROM media_info WHERE hash = ?1")?;
    let mut media_info: MediaInfo = statement.query_row(params![hash], |row| {
        Ok(MediaInfo {
            hash: hash.to_string(),
            p_hash: row.get(1)?,
            mime: row.get(2)?,
            date_registered: row.get(3)?,
            size: row.get(4)?,
            tags: vec![],
        })
    })?;

    let mut statement = conn.prepare("SELECT tag FROM media_tags WHERE hash = ?1")?;
    let rows = statement.query_map(params![hash], |row| row.get(0))?;

    for tag_res in rows {
        if let Ok(tagstring) = tag_res {
            media_info.tags.push(Tag::from_tagstring(&tagstring));
        }
    }

    Ok(media_info)
}

pub fn clear_tags(config: Arc<Config>, hash: &String) -> Result<()> {
    let conn = initialize_database_connection(&config.path.database()?)?;
    conn.execute("DELETE FROM media_tags WHERE hash = ?1", params![hash])?;
    Ok(())
}

pub fn delete_media(config: Arc<Config>, hash: &String) -> Result<()> {
    let conn = initialize_database_connection(&config.path.database()?)?;
    conn.execute("DELETE FROM media_info WHERE hash = ?1", params![hash])?;
    conn.execute("DELETE FROM media_bytes WHERE hash = ?1", params![hash])?;
    conn.execute("DELETE FROM media_tags WHERE hash = ?1", params![hash])?;
    conn.execute("DELETE FROM media_links WHERE hash = ?1", params![hash])?;
    conn.execute("DELETE FROM thumbnail_cache WHERE hash = ?1", params![hash])?;
    Ok(())
}

pub fn resolve_tags(config: Arc<Config>, tags: &Vec<Tag>) -> Result<Vec<Tag>> {
    let conn = initialize_database_connection(&config.path.database()?)?;
    //TODO: protect against circular impls
    // let inital_tags_data: Vec<TagData> = tags.iter().map(|tag| load_tag_data_with_conn(&conn, tag)).collect::<Result<Vec<TagData>>>()?;

    fn resolve_tags(conn: &Connection, tags: &Vec<Tag>, mut was_aliased_tagstrings: Vec<String>) -> Result<Vec<Tag>> {
        let inital_tags_data = tags
            .iter()
            .map(|tag| load_tag_data_with_conn(&conn, tag))
            .collect::<Result<Vec<TagData>>>()?;
        let add_tag = |new_tag: &Tag, resolved_tags: &mut Vec<Tag>, is_resolved: Option<&mut bool>| {
            // let n_new_tag = new_tag.noneified();
            let is_already_included = resolved_tags.iter().any(|tag| (tag.to_tagstring() == new_tag.to_tagstring()));
            if !is_already_included {
                resolved_tags.push(new_tag.clone());
                if let Some(is_resolved) = is_resolved {
                    *is_resolved = false;
                }
            }
        };
        let was_aliased = |tagstring: &String, was_aliased_tagstrings: &Vec<String>| -> bool {
            was_aliased_tagstrings.iter().any(|aliased_tagstring| aliased_tagstring == tagstring)
        };
        let mut resolved_tags = vec![];
        let mut is_resolved = true;
        for tag_data in &inital_tags_data {
            // let mut was_aliased = false;
            for link in &tag_data.links {
                let is_from_tag = link.from_tagstring == tag_data.tag.to_tagstring();
                let to_tag = Tag::from_tagstring(&link.to_tagstring);
                if (link.link_type == TagLinkType::Implication) && is_from_tag {
                    // resolved_tags.push(Tag::from_tagstring(&link.to_tagstring));
                    // is_resolved = false;
                    let is_already_included_in_initial = inital_tags_data
                        .iter()
                        .any(|tag_data| (tag_data.tag.to_tagstring() == to_tag.to_tagstring()));
                    let was_included_in_inital = was_aliased(&link.to_tagstring, &was_aliased_tagstrings); //was_aliased_tagstrings.iter().any(|tagstring| tagstring == &link.to_tagstring);
                    if !(is_already_included_in_initial || was_included_in_inital) {
                        // prevents circular imps
                        add_tag(&to_tag, &mut resolved_tags, Some(&mut is_resolved));
                    }
                }
                if (link.link_type == TagLinkType::Alias) && is_from_tag {
                    add_tag(&to_tag, &mut resolved_tags, Some(&mut is_resolved));
                    was_aliased_tagstrings.push(tag_data.tag.to_tagstring());
                    // was_aliased = true;
                }
            }
            if !was_aliased(&tag_data.tag.to_tagstring(), &was_aliased_tagstrings) {
                add_tag(&tag_data.tag, &mut resolved_tags, None);
                // resolved_tags.push(tag_data.tag.clone())
            }
        }
        if !is_resolved {
            resolved_tags = resolve_tags(conn, &resolved_tags, was_aliased_tagstrings)?;
        }

        Ok(resolved_tags)
    }
    // Ok(())
    // todo!()
    resolve_tags(&conn, tags, vec![])
}
// pub fn does_tag

pub fn does_link_exist(config: Arc<Config>, link: &TagLink) -> Result<bool> {
    let conn = initialize_database_connection(&config.path.database()?)?;

    let mut statement = conn.prepare("SELECT 1 FROM tag_links WHERE type = ?1 AND from_tag = ?2 AND to_tag = ?3")?;
    let exists = statement.exists(params![link.link_type.to_string(), link.from_tagstring, link.to_tagstring])?;

    Ok(exists)
}

pub fn does_tagstring_exist(config: Arc<Config>, tagstring: &String) -> Result<bool> {
    let does_exist = does_tag_exist(config.clone(), &Tag::from_tagstring(tagstring))?;
    Ok(does_exist)
}

pub fn does_tag_exist(config: Arc<Config>, tag: &Tag) -> Result<bool> {
    Ok(filter_to_unknown_tags(config, &vec![tag.clone()])?.len() == 0)
}

pub fn filter_to_unknown_tags(config: Arc<Config>, tags: &Vec<Tag>) -> Result<Vec<Tag>> {
    let conn = initialize_database_connection(&config.path.database()?)?;
    let mut not_exists = vec![];

    for tag in tags {
        let s_tag = tag.someified();
        // println!("{s_tag:?}");
        let mut statement = conn.prepare("SELECT 1 FROM tag_info WHERE name = ?1 AND namespace = ?2")?;
        let exists = statement.exists(params![s_tag.name, s_tag.namespace])?;
        if !exists {
            not_exists.push(tag.clone())
        }
    }
    Ok(not_exists)
}

pub fn set_tags(config: Arc<Config>, hash: &String, tags: &Vec<Tag>) -> Result<Vec<Tag>> {
    let resolved_tags = resolve_tags(Arc::clone(&config), tags)?;
    let conn = initialize_database_connection(&config.path.database()?)?;
    clear_tags(config, hash)?;
    for tag in resolved_tags.iter() {
        conn.execute("DELETE FROM media_tags WHERE hash = ?1 AND tag = ?2", params![hash, tag.to_tagstring()])?;
        conn.execute("INSERT INTO media_tags (hash, tag) VALUES (?1, ?2)", params![hash, tag.to_tagstring()])?;
    }
    Ok(resolved_tags)
}
pub fn get_all_hashes(config: Arc<Config>) -> Result<Vec<String>> {
    let db_path = config.path.database()?;
    let conn = initialize_database_connection(&db_path)?;
    let mut statement = conn.prepare("SELECT hash FROM media_info")?;
    let rows = statement.query_map([], |row| row.get(0))?;
    let mut hashes: Vec<String> = Vec::new();
    for hash_result in rows {
        hashes.push(hash_result?);
    }
    Ok(hashes)
}
pub fn get_links_of_hash(config: Arc<Config>, hash: &String) -> Result<Vec<i32>> {
    let conn = initialize_database_connection(&config.path.database()?)?;
    let mut statement = conn.prepare("SELECT DISTINCT id FROM media_links WHERE hash = ?1")?;
    let rows = statement.query_map(params![hash], |row| row.get(0))?;
    let mut link_ids: Vec<i32> = Vec::new();
    for id_result in rows {
        link_ids.push(id_result?);
    }
    Ok(link_ids)
}
pub fn get_hashes_of_link(config: Arc<Config>, link_id: i32) -> Result<Vec<String>> {
    let conn = initialize_database_connection(&config.path.database()?)?;
    let mut statement = conn.prepare("SELECT hash FROM media_links WHERE id = ?1")?;
    let rows = statement.query_map(params![link_id], |row| row.get(0))?;
    let mut hashes: Vec<String> = Vec::new();
    for hash_result in rows {
        hashes.push(hash_result?);
    }
    Ok(hashes)
}

pub fn delete_tag(config: Arc<Config>, tag: &Tag) -> Result<()> {
    let conn = initialize_database_connection(&config.path.database()?)?;

    let s_tag = tag.someified();
    conn.execute(
        "DELETE FROM tag_info WHERE name = ?1 AND namespace = ?2",
        params![s_tag.name, s_tag.namespace],
    )?;
    conn.execute("DELETE FROM tag_links WHERE from_tag = ?1", params![s_tag.to_tagstring()])?;
    conn.execute("DELETE FROM tag_links WHERE to_tag = ?1", params![s_tag.to_tagstring()])?;

    Ok(())
}

pub fn delete_link(config: Arc<Config>, link: &TagLink) -> Result<()> {
    let conn = initialize_database_connection(&config.path.database()?)?;

    conn.execute(
        "DELETE FROM tag_links WHERE type = ?1 AND from_tag = ?2 AND to_tag = ?3 ",
        params![link.link_type.to_string(), link.from_tagstring, link.to_tagstring],
    )?;

    Ok(())
}

pub fn register_tag(config: Arc<Config>, tag: &Tag) -> Result<()> {
    let conn = initialize_database_connection(&config.path.database()?)?;

    let s_tag = tag.someified();
    conn.execute(
        "DELETE FROM tag_info WHERE name = ?1 AND namespace = ?2",
        params![s_tag.name, s_tag.namespace],
    )?;
    conn.execute(
        "INSERT INTO tag_info (name, namespace, description)
            VALUES (?1, ?2, ?3)",
        // params![tag.name, tag.namespace.as_ref().unwrap_or(&"".to_string()), tag.description.as_ref().unwrap_or(&"".to_string())],
        params![
            s_tag.name,
            s_tag.namespace.as_ref().unwrap_or(&"".to_string()),
            s_tag.description.as_ref().unwrap_or(&"".to_string())
        ],
    )?;

    Ok(())
}

pub fn register_tag_link(config: Arc<Config>, link: &TagLink) -> Result<()> {
    let conn = initialize_database_connection(&config.path.database()?)?;
    conn.execute(
        "DELETE FROM tag_links WHERE type = ?1 AND from_tag = ?2 AND to_tag = ?3 ",
        params![link.link_type.to_string(), link.from_tagstring, link.to_tagstring],
    )?;
    conn.execute(
        "INSERT INTO tag_links (type, from_tag, to_tag)
            VALUES (?1, ?2, ?3)",
        params![link.link_type.to_string(), link.from_tagstring, link.to_tagstring],
    )?;

    Ok(())
}

pub fn get_all_tag_data(config: Arc<Config>) -> Result<Vec<TagData>> {
    let conn = initialize_database_connection(&config.path.database()?)?;
    let mut statement = conn.prepare("SELECT * FROM tag_info")?;

    let tag_results = statement.query_map([], |row| {
        Ok(Tag {
            name: row.get(0)?,
            namespace: row.get(1)?,
            description: row.get(2)?,
        })
    })?;

    let mut all_tag_data: Vec<TagData> = vec![];
    for tag_result in tag_results {
        if let Ok(tag) = tag_result {
            all_tag_data.push(load_tag_data_with_conn(&conn, &tag)?);
        }
    }

    Ok(all_tag_data)
}

pub fn register_media(
    config: Arc<Config>,
    bytes: &[u8],
    filekind: Option<infer::Type>,
    linking_dir: Option<String>,
    dir_link_map: Arc<Mutex<HashMap<String, i32>>>,
) -> ImportationStatus {
    fn register(
        config: Arc<Config>,
        bytes: &[u8],
        filekind: Option<infer::Type>,
        linking_dir: Option<String>,
        dir_link_map: Arc<Mutex<HashMap<String, i32>>>,
    ) -> Result<ImportationStatus> {
        // println!("got {} kB for register", bytes.len() / 1000);
        let hasher_config = HasherConfig::new().hash_alg(HashAlg::DoubleGradient);
        let hasher = hasher_config.to_hasher();
        let db_path = config.path.database()?;
        let conn = initialize_database_connection(&db_path)?;

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
        if exists {
            return Ok(ImportationStatus::Duplicate);
        }

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
                        } else {
                            // new link_id
                            let next_id: i32 = conn.query_row("SELECT IFNULL(MAX(id), 0) + 1 FROM media_links ", [], |row| row.get(0))?;
                            conn.execute(
                                "INSERT INTO media_links (id, hash)
                                    VALUES (?1, ?2)",
                                params![next_id, sha_hash],
                            )?;

                            dir_link_map.insert(linking_dir, next_id);
                        }
                    }
                }

                return Ok(ImportationStatus::Success);
            }
            Err(error) => {
                // if (let rusqlite::Error::SqliteFailure(e, _) = error) && e.code == rusqlite::ErrorCode::ConstraintViolation { waiting for rust 1.62 :(
                if let rusqlite::Error::SqliteFailure(e, _) = error {
                    if e.code == rusqlite::ErrorCode::ConstraintViolation {
                        return Ok(ImportationStatus::Duplicate);
                    }
                }
                return Ok(ImportationStatus::Fail(error.into()));
            }
        }
    }

    match register(config, bytes, filekind, linking_dir, dir_link_map) {
        Ok(status) => return status,
        Err(error) => return ImportationStatus::Fail(error),
    };
}

fn load_tag_data_with_conn(conn: &Connection, tag: &Tag) -> Result<TagData> {
    let mut count_stmt = conn.prepare("SELECT COUNT(*) FROM media_tags WHERE tag = ?1")?;
    let mut link_stmt = conn.prepare("SELECT * FROM tag_links WHERE from_tag = ?1 OR to_tag = ?1")?;
    let occurances: i32 = count_stmt.query_row(params![tag.to_tagstring()], |row| Ok(row.get(0)?))?;

    let link_results = link_stmt.query_map(params![tag.to_tagstring()], |row| {
        let link_type: String = row.get(0)?;
        Ok(TagLink {
            link_type: TagLinkType::from(link_type),
            from_tagstring: row.get(1)?,
            to_tagstring: row.get(2)?,
        })
    })?;

    let mut tag_data = TagData {
        tag: tag.noneified(),
        occurances,
        links: vec![],
    };

    for link_result in link_results {
        if let Ok(link) = link_result {
            tag_data.links.push(link);
        }
    }
    Ok(tag_data)
}
