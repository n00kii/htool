// use crate::gallery::gallery_ui::EntrySearch;

// use crate::tags::tags::Tag;
// use crate::tags::tags::TagData;
// use crate::tags::tags::TagLink;
// use crate::tags::tags::TagLinkType;

use crate::tags::Tag;
use crate::tags::TagData;
use crate::tags::TagLink;
use crate::tags::TagLinkType;
use crate::ui::gallery_ui::EntrySearch;
use crate::ui::preview_ui::MediaPreview;

use super::ui;
use super::Config;

use anyhow::anyhow;
use anyhow::{Context, Result};
use egui_extras::RetainedImage;
use egui_video::VideoStream;
use image::DynamicImage;
use image::RgbaImage;
use image::{imageops, ImageBuffer, Rgba};
use image_hasher::{HashAlg, HasherConfig};
// use infer;
use once_cell::sync::OnceCell;

use parking_lot::lock_api::RawMutex;
use parking_lot::RwLock;
use poll_promise::Sender;
use r2d2::Pool;
use r2d2::PooledConnection;
use r2d2_sqlite::SqliteConnectionManager;
use rand::distributions::DistString;
use rusqlite::ErrorCode;
use rusqlite::Row;

use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::fmt;
use std::fmt::Display;

use std::fs;
use std::hash;
use std::io::Cursor;
use std::mem::discriminant;
use std::path::PathBuf;
use std::sync::Arc;
// use std::sync::Mutex;
use parking_lot::Mutex;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

const DATABASE_WORKERS_PER_TASK: u32 = 5;
static DATABASE_KEY: RwLock<String> = parking_lot::const_rwlock(String::new());
const GENERIC_RUSQLITE_ERROR: rusqlite::Error = rusqlite::Error::InvalidQuery;
static POOLS: OnceCell<Pool<SqliteConnectionManager>> = OnceCell::new();

pub struct DatabaseInfo {
    pub thumbnail_cache_size: usize,
    pub media_bytes_size: usize,
    pub entry_info_size: usize,
    pub entry_tags_size: usize,
    pub media_links_size: usize,
    pub tag_info_size: usize,
    pub is_unencrypted: bool,
    pub current_key: String,
    pub thumbnail_cache_count: usize,
    pub tag_info_count: usize,
    pub entry_info_count: usize,
    pub media_bytes_count: usize,
    pub tag_links_size: usize,
    pub entry_tags_count: usize,
}

pub fn load_database_info() -> Result<DatabaseInfo> {
    // todo!()
    let conn = initialize_database_connection()?;
    let mut table_size_stmt = conn.prepare("SELECT SUM(pgsize) FROM dbstat WHERE name = ?1")?;
    // let mut table_count_stmt = conn.prepare("SELECT COUNT(*) from ?1")?;
    let mut get_table_size = |table_name: &str| table_size_stmt.query_row(params![table_name], |row| row.get(0));
    // let mut get_table_count = |table_name: &str| table_count_stmt.query_row(params![table_name], |row| row.get(0));

    let thumbnail_cache_size: usize = get_table_size("thumbnail_cache")?;
    // let media_bytes_size: usize = get_table_size("media_bytes")?; // this takes too long
    let entry_info_size: usize = get_table_size("entry_info")?;
    let entry_tags_size: usize = get_table_size("entry_tags")?;
    let media_links_size: usize = get_table_size("media_links")?;
    let tag_info_size: usize = get_table_size("tag_info")?;
    let tag_links_size: usize = get_table_size("tag_links")?;
    let media_bytes_size: usize = fs::metadata(Config::global().path.database()?)?.len() as usize
        - entry_info_size
        - entry_tags_size
        - tag_info_size
        - tag_links_size
        - media_links_size
        - thumbnail_cache_size;

    let media_bytes_count: usize = conn.query_row("SELECT COUNT(*) from media_bytes", [], |row| row.get(0))?;
    let entry_info_count: usize = conn.query_row("SELECT COUNT(*) from entry_info", [], |row| row.get(0))?;
    let thumbnail_cache_count: usize = conn.query_row("SELECT COUNT(*) from thumbnail_cache", [], |row| row.get(0))?;
    let tag_info_count: usize = conn.query_row("SELECT COUNT(*) from tag_info", [], |row| row.get(0))?;
    let entry_tags_count: usize = conn.query_row("SELECT COUNT(*) from entry_tags", [], |row| row.get(0))?;
    // let media_bytes_count: usize = get_table_count("media_bytes")?;
    // let entry_info_count: usize = get_table_count("entry_info")?; // conn.query_row("SELECT COUNT(*) from entry_info", [], |row| row.get(0))?;
    // let thumbnail_cache_count: usize = get_table_count("thumbnail_cache")?; // conn.query_row("SELECT COUNT(*) from thumbnail_cache", [], |row| row.get(0))?;
    // let tag_info_count: usize = get_table_count("tag_info")?; // conn.query_row("SELECT COUNT(*) from tag_info", [], |row| row.get(0))?;
    // let entry_tags_count: usize = get_table_count("entry_tags")?; // conn.query_row("SELECT COUNT(*) from entry_tags", [], |row| row.get(0))?;

    Ok(DatabaseInfo {
        entry_info_count,
        media_bytes_count,
        tag_links_size,
        thumbnail_cache_count,
        tag_info_count,
        thumbnail_cache_size,
        media_bytes_size,
        entry_info_size,
        entry_tags_count,
        entry_tags_size,
        media_links_size,
        tag_info_size,
        is_unencrypted: is_database_unencrypted()?,
        current_key: get_database_key(),
    })
}
pub fn get_conn_pool() -> &'static Pool<SqliteConnectionManager> {
    POOLS.get().expect("uninitialized db")
}

pub fn flush_thumbnail_cache() -> Result<()> {
    let conn = initialize_database_connection()?;
    conn.execute("DROP TABLE thumbnail_cache", [])?;
    Ok(())
}
pub fn flush_media_bytes() -> Result<()> {
    let conn = initialize_database_connection()?;
    conn.execute("DROP TABLE media_bytes", [])?;
    Ok(())
}
pub fn flush_entry_info_media_links() -> Result<()> {
    let conn = initialize_database_connection()?;
    conn.execute("DROP TABLE entry_info", [])?;
    conn.execute("DROP TABLE media_links", [])?;
    Ok(())
}
pub fn flush_tag_definitions() -> Result<()> {
    let conn = initialize_database_connection()?;
    conn.execute("DROP TABLE tag_info", [])?;
    conn.execute("DROP TABLE tag_links", [])?;
    Ok(())
}
pub fn flush_entry_tags() -> Result<()> {
    let conn = initialize_database_connection()?;
    conn.execute("DROP TABLE entry_tags", [])?;
    Ok(())
}

pub fn generate_conn_pool() -> Result<Pool<SqliteConnectionManager>> {
    let manager = SqliteConnectionManager::file(Config::global().path.database().context("failed to initialize database manager")?)
        .with_init(move |c| c.execute_batch(&format!("PRAGMA key = '{}';", get_database_key())));

    let connection_manager = r2d2::Pool::builder()
        .max_size(DATABASE_WORKERS_PER_TASK)
        .build(manager)
        .context("failed to create conn pool")?;

    Ok(connection_manager)
}

pub fn get_database_key() -> String {
    DATABASE_KEY.read().to_string() //.as_deref().map(|s| s.to_string())
}

pub fn set_db_key(new_key: &String) {
    let mut db_key = DATABASE_KEY.write();
    *db_key = new_key.clone();
}

fn apply_database_key_to_conn(conn: &Connection, key: &String) -> Result<()> {
    conn.pragma_update(None, "key", key)?;
    Ok(())
}

pub fn is_connection_unlocked(conn: &Connection) -> Result<bool> {
    match conn.execute("SELECT COUNT(*) FROM sqlite_master", []) {
        Ok(_) => Ok(true),
        Err(e) => {
            if matches!(e.sqlite_error_code(), Some(ErrorCode::NotADatabase)) {
                Ok(false)
            } else {
                Ok(true)
            }
        }
    }
}

pub fn try_unlock_database_with_key(key: &String) -> Result<bool> {
    let conn = open_database_connection()?;
    apply_database_key_to_conn(&conn, key)?;
    is_connection_unlocked(&conn)
}

pub struct DataRequest<T> {
    pub entry_id: EntryId,
    pub sender: Sender<Result<T>>,
}
pub struct CompleteDataRequest {
    pub info_request: DataRequest<EntryInfo>,
    pub preview_request: DataRequest<MediaPreview>,
}

pub struct RegistrationForm {
    pub bytes: Arc<Vec<u8>>,
    pub mimetype: mime_guess::MimeGuess,
    pub importation_result_sender: Sender<ImportationStatus>,
    pub linking_dir: Option<String>,
    pub linking_value: Option<i32>,
    pub dir_link_map: Arc<Mutex<HashMap<String, i32>>>,
}

#[derive(Debug)]
pub enum ImportationStatus {
    Pending,
    Success,
    Duplicate,
    Fail(anyhow::Error),
}

impl PartialEq for ImportationStatus {
    fn eq(&self, other: &Self) -> bool {
        discriminant(self) == discriminant(other)
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum EntryId {
    MediaEntry(String),
    PoolEntry(i32),
}

impl EntryId {
    pub fn is_media_entry_id(&self) -> bool {
        matches!(self, EntryId::MediaEntry(_))
    }
    pub fn is_pool_entry_id(&self) -> bool {
        matches!(self, EntryId::PoolEntry(_))
    }
    pub fn as_media_entry_id(&self) -> Option<&String> {
        if let EntryId::MediaEntry(hash) = self {
            Some(hash)
        } else {
            None
        }
    }
    pub fn as_pool_entry_id(&self) -> Option<&i32> {
        if let EntryId::PoolEntry(link_id) = self {
            Some(link_id)
        } else {
            None
        }
    }
}

impl Display for EntryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EntryId::MediaEntry(hash) => write!(f, "{hash}"),
            EntryId::PoolEntry(link_id) => write!(f, "{link_id}"),
        }
    }
}
#[derive(PartialEq, Clone, Debug)]
pub enum EntryInfo {
    MediaEntry(MediaInfo),
    PoolEntry(PoolInfo),
}

impl EntryInfo {
    pub fn details(&self) -> &EntryDetails {
        match self {
            EntryInfo::MediaEntry(media_info) => &media_info.details,
            EntryInfo::PoolEntry(pool_info) => &pool_info.details,
        }
    }
    pub fn details_mut(&mut self) -> &mut EntryDetails {
        match self {
            EntryInfo::MediaEntry(media_info) => &mut media_info.details,
            EntryInfo::PoolEntry(pool_info) => &mut pool_info.details,
        }
    }
    pub fn entry_id(&self) -> &EntryId {
        &self.details().id
    }
    pub fn is_movie(&self) -> bool {
        if let EntryInfo::MediaEntry(media_info) = self {
            if vec!["video/webm", "video/mp4", "image/gif"].contains(&media_info.mime.as_str()) {
                return true;
            }
        }
        false
    }
    pub fn passes_entry_search(&self, search: &EntrySearch) -> bool {
        if !search.is_valid {
            return false;
        }
        let fails_opt_xor = |opt: Option<bool>, self_value: bool| -> bool {
            if let Some(opt_value) = opt {
                if opt_value ^ self_value {
                    return true;
                }
            }
            false
        };
        if fails_opt_xor(search.is_media, self.entry_id().as_media_entry_id().is_some())
            || fails_opt_xor(search.is_pool, self.entry_id().as_pool_entry_id().is_some())
            || fails_opt_xor(search.is_bookmarked, self.details().is_bookmarked)
            || fails_opt_xor(search.is_independant, self.details().is_independant)
        {
            return false;
        }
        let score = self.details().score;
        if let Some(exact_score) = search.score_exact {
            if score != exact_score {
                return false;
            }
        }
        if let Some((min_score, inclusive)) = search.score_min {
            if (score < min_score) || ((score == min_score) && !inclusive) {
                return false;
            }
        }
        if let Some((max_score, inclusive)) = search.score_max {
            if (score > max_score) || ((score == max_score) && !inclusive) {
                return false;
            }
        }
        for tags in &search.not_relations {
            if !self.details().not_includes_any_tags(tags) {
                return false;
            }
        }
        for tags in &search.and_relations {
            if !self.details().includes_all_tags(tags) {
                return false;
            }
        }
        for tags in &search.or_relations {
            if !self.details().includes_any_tag(tags) {
                return false;
            }
        }

        true
    }
}

static DB_WRITE_LOCK: Mutex<()> = parking_lot::const_mutex(());

#[derive(Clone, Debug)]
pub struct MediaInfo {
    pub mime: String,
    pub p_hash: Option<String>,
    pub links: Vec<i32>,
    pub details: EntryDetails,
}

impl PartialEq for MediaInfo {
    fn eq(&self, other: &Self) -> bool {
        self.details == other.details
    }
}

#[derive(Clone, Debug)]
pub struct PoolInfo {
    pub hashes: Vec<String>,
    pub details: EntryDetails,
}

impl PartialEq for PoolInfo {
    fn eq(&self, other: &Self) -> bool {
        self.details == other.details
    }
}

#[derive(Clone, Debug)]
pub struct EntryDetails {
    pub id: EntryId,
    pub tags: Vec<Tag>,
    pub size: i64,
    pub date_registered: i64,
    pub score: i64,
    pub is_bookmarked: bool,
    pub is_independant: bool,
}

impl PartialEq for EntryDetails {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl EntryDetails {
    pub fn includes_tag(&self, tag: &Tag) -> bool {
        self.tags.iter().any(|included_tag| included_tag == tag)
    }
    pub fn includes_any_tag(&self, tags: &Vec<Tag>) -> bool {
        for tag in tags {
            if self.includes_tag(tag) {
                return true;
            }
        }
        false
    }
    pub fn includes_all_tags(&self, tags: &Vec<Tag>) -> bool {
        for tag in tags {
            if !self.includes_tag(tag) {
                return false;
            }
        }
        true
    }
    pub fn not_includes_any_tags(&self, tags: &Vec<Tag>) -> bool {
        for tag in tags {
            if self.includes_tag(tag) {
                return false;
            }
        }
        true
    }
}

// todo: move stuff out of struct
pub fn generate_media_thumbnail(image_data: &[u8], is_movie: bool) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>> {
    let thumbnail_size = Config::global().ui.thumbnail_resolution as u32;
    let image = if is_movie {
        let ctx = egui::Context::default();
        let streamer = VideoStream::new_from_bytes(&ctx, image_data)?;
        let next_frame = streamer.stream_decoder.lock().unwrap().recieve_next_packet_until_frame()?;
        let pixels = next_frame
            .pixels
            .iter()
            .flat_map(|c32| [c32.r(), c32.g(), c32.b(), c32.a()])
            .collect::<Vec<u8>>();
        RgbaImage::from_raw(streamer.width, streamer.height, pixels).context("failed to make image")?
    } else {
        image::load_from_memory(image_data)?.to_rgba8()
    };
    let (w, h) = (image.width(), image.height());
    let image_cropped = imageops::crop_imm(
        &image,
        if h > w { 0 } else { (w - h) / 2 },
        if w > h { 0 } else { (h - w) / 2 },
        if h > w { w } else { h },
        if w > h { h } else { w },
    )
    .to_image();
    let thumbnail = imageops::thumbnail(&image_cropped, thumbnail_size, thumbnail_size);
    Ok(thumbnail)
}

// mod constants

// https://math.stackexchange.com/questions/4489146/filling-a-square-with-squares-along-the-diagonal
pub fn generate_pool_thumbnail(constituent_thumbnails: &Vec<ImageBuffer<Rgba<u8>, Vec<u8>>>) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>> {
    // https://www.desmos.com/calculator/moriiungym for tuning
    let thumbnail_size = Config::global().ui.thumbnail_resolution as u32;
    let distance_factor = 0.3; //1 = thumbnails half size away from each other, 0 = thumbnails on top of each other
    let size_factor = 0.3; // 0 = normal size; 1 = thumbnail_size size; >0.5 = constituent thumbnails clip out of bounds

    // size of the mini thumbnails

    let size_factor_scaled = 0.5 * (constituent_thumbnails.len() as f64) * size_factor + 1.0;
    let size_factor_positional_offset = thumbnail_size as f64 * distance_factor * (size_factor_scaled - 1.0) * 0.5;

    let constituent_thumbnail_size = ((2.0 * thumbnail_size as f64) / (constituent_thumbnails.len() as f64 + 1.0) * size_factor_scaled) as u32;
    // ?dbg!(distance_factor, thumbnail_size, constituent_thumbnail_size, constituent_thumbnails.len());
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

pub fn load_thumbnail_with_conn(conn: &Connection, entry_id: &EntryId) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>> {
    // let conn = initialize_database_connection()?;
    puffin::profile_scope!("load_thumbnail");

    let mut statement = conn.prepare("SELECT bytes FROM thumbnail_cache WHERE hash = ?1 OR link_id = ?2")?;
    let bytes_res: Result<Vec<u8>, rusqlite::Error> =
        statement.query_row(params![entry_id.as_media_entry_id(), entry_id.as_pool_entry_id()], |row| row.get(0));

    match bytes_res {
        Ok(bytes) => {
            puffin::profile_scope!("load_mem");

            let image = image::load_from_memory(&bytes)?;
            let image_buffer = image.into_rgba8();
            // if let Some(image_buffer) = ImageBuffer::from_vec(100, 100, bytes) {

            //     return Ok(image_buffer);
            // } else {
            //     return Err(anyhow!("hmmm"))
            // }
            // dbg!(bytes.len(), image_buffer.len());
            return Ok(image_buffer);
        }
        Err(error) => {
            // dbg!(&error);
            if error == rusqlite::Error::QueryReturnedNoRows {
                match entry_id {
                    EntryId::MediaEntry(hash) => {
                        let mut statement = conn.prepare("SELECT mime FROM entry_info WHERE hash = ?1")?;
                        let entry_info = get_entry_info_with_conn(conn, entry_id)?;
                        let mime_type: Option<String> = statement.query_row(params![hash], |row| row.get("mime"))?;
                        let bytes = get_media_bytes(&hash)?;
                        // if mime_type.starts_with("image") {
                        let thumbnail_res = generate_media_thumbnail(&bytes, entry_info.is_movie());
                        match thumbnail_res {
                            Ok(thumbnail) => {
                                let mut thumbnail_bytes: Vec<u8> = Vec::new();
                                let mut writer = Cursor::new(&mut thumbnail_bytes);
                                thumbnail.write_to(&mut writer, image::ImageOutputFormat::Png)?;
                                // let thumbnail_bytes = thumbnail.as_ref();
                                let lock = DB_WRITE_LOCK.lock();
                                conn.execute(
                                    "INSERT INTO thumbnail_cache (hash, bytes)
                                    VALUES (?1, ?2)",
                                    params![hash, thumbnail_bytes],
                                )?;
                                drop(lock);
                                return Ok(thumbnail);
                            }
                            Err(_e) => {
                                return Err(anyhow::Error::msg(format!(
                                    "can't create thumbnail for {}",
                                    mime_type.unwrap_or("unknown type".to_string())
                                )))
                            }
                        }
                    }
                    EntryId::PoolEntry(link_id) => {
                        let max_constituent_thumbnails = 3;
                        let mut hashes_of_link = get_hashes_of_media_link(link_id)?;
                        hashes_of_link.truncate(max_constituent_thumbnails);
                        hashes_of_link.reverse();

                        if hashes_of_link.len() == 0 {
                            return Err(anyhow::Error::msg("no hashes in link"));
                        }
                        let mut constituent_thumbnails = hashes_of_link
                            .into_iter()
                            .filter_map(|hash| load_thumbnail_with_conn(conn, &EntryId::MediaEntry(hash)).ok())
                            .collect::<Vec<_>>();

                        if constituent_thumbnails.len() == 0 {
                            return Err(anyhow::Error::msg("couldnt load any thumbnails of hashes of link"));
                        } else if constituent_thumbnails.len() == 1 {
                            return Ok(constituent_thumbnails.remove(0));
                        }

                        let thumbnail = generate_pool_thumbnail(&constituent_thumbnails)?;
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
                }
            }
            // todo!()
            return Err(error.into());
        }
    }
}

trait FallibleSender {
    fn fail(self, error: anyhow::Error);
}

fn initialize_database_connection_with_senders<T>(senders: T) -> Result<(Connection, T)>
where
    T: FallibleSender,
{
    match initialize_database_connection() {
        Ok(c) => Ok((c, senders)),
        Err(e) => {
            senders.fail(anyhow::Error::msg(e.to_string()));
            return Err(e);
        }
    }
}

fn open_database_connection() -> Result<Connection> {
    Ok(Connection::open(&Config::global().path.database()?)?)
}

fn setup_databaste_with_conn(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS entry_info (
                hash TEXT,
                link_id INTEGER,
                perceptual_hash TEXT,
                mime TEXT,
                date_registered INTEGER,
                is_bookmarked INTEGER DEFAULT 0,
                score INTEGER DEFAULT 0,
                size INTEGER DEFAULT 0,
                is_independant INTEGER DEFAULT 1
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
        "CREATE TABLE IF NOT EXISTS entry_tags (
                hash TEXT,
                link_id INTEGER,
                tag TEXT,
                UNIQUE (hash, link_id, tag)
            )",
        [],
    )?;

    conn.execute(
        // vertical array
        "CREATE TABLE IF NOT EXISTS media_links (
                link_id INTEGER NOT NULL,
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
    Ok(())
}

pub fn is_database_unencrypted() -> Result<bool> {
    let conn = open_database_connection()?;
    apply_database_key_to_conn(&conn, &String::new())?;
    is_connection_unlocked(&conn)
}

pub fn initialize_database_connection() -> Result<Connection> {
    let conn = open_database_connection()?; //Connection::open(&Config::global().path.database()?)?;
    apply_database_key_to_conn(&conn, &get_database_key())?;
    setup_databaste_with_conn(&conn)?;
    Ok(conn)
}

pub fn get_media_bytes(hash: &String) -> Result<Vec<u8>> {
    let conn = initialize_database_connection()?;
    get_media_bytes_with_conn(&conn, hash)
}

pub fn get_media_bytes_with_conn(conn: &Connection, hash: &String) -> Result<Vec<u8>> {
    let mut statement = conn.prepare("SELECT bytes FROM media_bytes WHERE hash = ?1")?;
    let bytes: Vec<u8> = statement.query_row(params![hash], |row| row.get(0))?;
    Ok(bytes)
}

fn construct_entry_info_with_row(entry_id: &EntryId, row: &Row) -> Result<EntryInfo> {
    let details = EntryDetails {
        id: entry_id.clone(),
        date_registered: row.get("date_registered")?,
        score: row.get("score")?,
        is_bookmarked: row.get("is_bookmarked")?,
        is_independant: row.get("is_independant")?,
        size: row.get("size")?,
        tags: vec![],
    };

    let entry_info = match entry_id {
        EntryId::MediaEntry(_) => EntryInfo::MediaEntry(MediaInfo {
            links: vec![],
            p_hash: row.get("perceptual_hash")?,
            mime: row.get("mime")?,
            details,
        }),
        EntryId::PoolEntry(_) => EntryInfo::PoolEntry(PoolInfo { details, hashes: vec![] }),
    };

    Ok(entry_info)
}

fn fill_entry_info_tags_with_conn(conn: &Connection, entry_info: &mut EntryInfo) -> Result<()> {
    let id_param = params![entry_info.entry_id().as_media_entry_id(), entry_info.entry_id().as_pool_entry_id()];
    // println!("id_param: {:?} {:?}", entry_info.entry_id().as_media_entry_id(), entry_info.entry_id().as_pool_entry_id());
    let mut tags_statement = conn.prepare("SELECT tag FROM entry_tags WHERE hash = ?1 OR link_id = ?2")?;
    let tag_rows = tags_statement.query_map(id_param, |row| row.get(0))?;
    for tag_res in tag_rows {
        // dbg!(&tag_res);
        if let Ok(tagstring) = tag_res {
            entry_info.details_mut().tags.push(Tag::from_tagstring(&tagstring));
        }
    }

    Ok(())
}

fn fill_media_info_wth_conn(conn: &Connection, entry_info: &mut EntryInfo) -> Result<()> {
    if let EntryInfo::MediaEntry(media_info) = entry_info {
        media_info.links = get_media_links_of_hash_with_conn(&conn, media_info.details.id.as_media_entry_id().unwrap())?;
    }
    Ok(())
}

fn fill_pool_info_with_conn(conn: &Connection, entry_info: &mut EntryInfo) -> Result<()> {
    if let EntryInfo::PoolEntry(pool_info) = entry_info {
        pool_info.hashes = get_hashes_of_media_link_with_conn(&conn, pool_info.details.id.as_pool_entry_id().unwrap())?;
        let mut sizes_stmt = conn.prepare("SELECT size FROM entry_info WHERE hash = ?1")?;
        let mut total_size = 0;
        for hash in &pool_info.hashes {
            let size: i64 = sizes_stmt.query_row(params![hash], |row| row.get(0))?;
            total_size += size;
        }
        pool_info.details.size = total_size;
    }
    Ok(())
}

pub fn entry_info_row_to_id(row: &Row) -> Result<EntryId, rusqlite::Error> {
    let hash: Option<String> = row.get("hash")?;
    let link_id: Option<i32> = row.get("link_id")?;
    if hash.is_some() {
        Ok(EntryId::MediaEntry(hash.unwrap()))
    } else if link_id.is_some() {
        Ok(EntryId::PoolEntry(link_id.unwrap()))
    } else {
        Err(GENERIC_RUSQLITE_ERROR)
    }
}

pub fn get_all_entry_ids_with_conn(conn: &Connection) -> Result<Vec<EntryId>> {
    let mut stmt = conn.prepare("SELECT hash, link_id FROM entry_info")?;
    let all_entry_ids = stmt
        .query_map([], |row| {
            let entry_id = entry_info_row_to_id(row)?;
            Ok(entry_id)
        })?
        .filter_map(|id_res| id_res.ok())
        .collect::<Vec<_>>();

    Ok(all_entry_ids)
}

pub fn get_all_entry_info() -> Result<Vec<EntryInfo>> {
    let conn = initialize_database_connection()?;
    get_all_entry_info_with_conn(&conn)
}

fn get_all_entry_info_with_conn(conn: &Connection) -> Result<Vec<EntryInfo>> {
    let mut entry_info_statement = conn.prepare("SELECT * FROM entry_info ORDER BY date_registered DESC")?;
    let all_entry_info = entry_info_statement
        .query_map([], |row| {
            let entry_id = entry_info_row_to_id(row)?;
            let mut entry_info = construct_entry_info_with_row(&entry_id, row).map_err(|_| GENERIC_RUSQLITE_ERROR)?;
            fill_entry_info_tags_with_conn(&conn, &mut entry_info).map_err(|_| GENERIC_RUSQLITE_ERROR)?;
            fill_media_info_wth_conn(&conn, &mut entry_info).map_err(|_| GENERIC_RUSQLITE_ERROR)?;
            fill_pool_info_with_conn(&conn, &mut entry_info).map_err(|_| GENERIC_RUSQLITE_ERROR)?;
            Ok(entry_info)
        })?
        .filter_map(|info_res| info_res.ok())
        .collect::<Vec<_>>();
    Ok(all_entry_info)
}

pub fn get_entry_info(entry_id: &EntryId) -> Result<EntryInfo> {
    let conn = initialize_database_connection()?;
    get_entry_info_with_conn(&conn, entry_id)
}

fn get_entry_info_with_conn(conn: &Connection, entry_id: &EntryId) -> Result<EntryInfo> {
    let mut entry_info_statement = conn.prepare("SELECT * FROM entry_info WHERE hash = ?1 OR link_id = ?2")?;
    let id_param = params![entry_id.as_media_entry_id(), entry_id.as_pool_entry_id()];

    let mut entry_info = entry_info_statement.query_row(id_param, |row| {
        construct_entry_info_with_row(entry_id, row).map_err(|_| GENERIC_RUSQLITE_ERROR)
    })?;

    fill_entry_info_tags_with_conn(&conn, &mut entry_info)?;
    fill_media_info_wth_conn(&conn, &mut entry_info)?;
    fill_pool_info_with_conn(&conn, &mut entry_info)?;

    Ok(entry_info)
}

pub fn set_media_link_values_in_order(link_id: &i32, hashes: Vec<String>) -> Result<()> {
    let mut conn = initialize_database_connection()?;
    let tx = conn.transaction()?;
    for (index, hash) in hashes.iter().enumerate() {
        set_media_link_value_with_conn(&tx, link_id, hash, index as i64)?;
    }
    tx.commit()?;
    Ok(())
}

pub fn set_score(entry_id: &EntryId, new_score: i64) -> Result<()> {
    let conn = initialize_database_connection()?;
    match entry_id {
        EntryId::MediaEntry(hash) => conn.execute("UPDATE entry_info SET score = ?1 WHERE hash = ?2", params![new_score, hash])?,
        EntryId::PoolEntry(link_id) => conn.execute("UPDATE entry_info SET score = ?1 WHERE link_id = ?2", params![new_score, link_id])?,
    };
    Ok(())
}

pub fn get_entries_with_tag(tag: &Tag) -> Result<Vec<EntryId>> {
    let conn = initialize_database_connection()?;
    let mut stmt = conn.prepare("SELECT hash, link_id FROM entry_tags WHERE tag = ?1")?;
    let id_results = stmt.query_map(params![tag.to_tagstring()], |row| entry_info_row_to_id(row))?;

    Ok(id_results.into_iter().filter_map(|id_res| id_res.ok()).collect::<Vec<_>>())
}

pub fn set_independance_with_conn(conn: &Connection, hash: &String, new_state: bool) -> Result<()> {
    conn.execute("UPDATE entry_info SET is_independant = ?1 WHERE hash = ?2", params![new_state, hash])?;
    Ok(())
}

pub fn set_bookmark(entry_id: &EntryId, new_state: bool) -> Result<()> {
    let conn = initialize_database_connection()?;
    match entry_id {
        EntryId::MediaEntry(hash) => conn.execute("UPDATE entry_info SET is_bookmarked = ?1 WHERE hash = ?2", params![new_state, hash])?,
        EntryId::PoolEntry(link_id) => conn.execute("UPDATE entry_info SET is_bookmarked = ?1 WHERE link_id = ?2", params![new_state, link_id])?,
    };
    Ok(())
}

pub fn clear_entry_tags_with_conn(conn: &Connection, entry_id: &EntryId) -> Result<()> {
    match entry_id {
        EntryId::MediaEntry(hash) => conn.execute("DELETE FROM entry_tags WHERE hash = ?1", params![hash])?,
        EntryId::PoolEntry(link_id) => conn.execute("DELETE FROM entry_tags WHERE link_id = ?1", params![link_id])?,
    };
    Ok(())
}

fn delete_entry_with_conn(conn: &Connection, entry_id: &EntryId) -> Result<()> {
    match entry_id {
        EntryId::MediaEntry(hash) => {
            conn.execute("DELETE FROM entry_info WHERE hash = ?1", params![hash])?;
            conn.execute("DELETE FROM media_bytes WHERE hash = ?1", params![hash])?;
            conn.execute("DELETE FROM entry_tags WHERE hash = ?1", params![hash])?;
            conn.execute("DELETE FROM media_links WHERE hash = ?1", params![hash])?;
            conn.execute("DELETE FROM thumbnail_cache WHERE hash = ?1", params![hash])?;
        }
        EntryId::PoolEntry(link_id) => {
            conn.execute("DELETE FROM entry_info WHERE link_id = ?1", params![link_id])?;
            conn.execute("DELETE FROM entry_tags WHERE link_id = ?1", params![link_id])?;
            conn.execute("DELETE FROM thumbnail_cache WHERE link_id = ?1", params![link_id])?;
            let hashes_of_link = get_hashes_of_media_link_with_conn(conn, link_id)?;
            conn.execute("DELETE FROM media_links WHERE link_id = ?1", params![link_id])?;
            for hash in hashes_of_link {
                match get_media_links_of_hash_with_conn(conn, &hash)?.as_slice() {
                    [single_id] => {
                        if single_id == link_id {
                            set_independance_with_conn(conn, &hash, true)?
                        }
                    }
                    [] => set_independance_with_conn(conn, &hash, true)?,
                    _ => (),
                }
            }
        }
    }

    Ok(())
}

pub fn delete_entry(entry_id: &EntryId) -> Result<()> {
    let mut conn = initialize_database_connection()?;
    let tx = conn.transaction()?;
    delete_entry_with_conn(&tx, entry_id)?;
    tx.commit()?;
    Ok(())
}

pub fn delete_link_and_linked(link_id: &i32) -> Result<()> {
    let mut conn = initialize_database_connection()?;
    let tx = conn.transaction()?;
    for hash in get_hashes_of_media_link_with_conn(&tx, link_id)? {
        delete_entry_with_conn(&tx, &EntryId::MediaEntry(hash))?;
    }
    delete_entry_with_conn(&tx, &EntryId::PoolEntry(*link_id))?;
    tx.commit()?;
    Ok(())
}

pub fn remove_media_from_link(link_id: &i32, hash: &String) -> Result<()> {
    let mut conn = initialize_database_connection()?;
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM media_links WHERE hash = ?1 AND link_id = ?2", params![hash, link_id])?;
    if get_media_links_of_hash_with_conn(&tx, hash)?.len() == 0 {
        set_independance_with_conn(&tx, &hash, true)?;
    }
    tx.commit()?;
    Ok(())
}

pub fn reresolve_tags_of_entries(entry_ids: &Vec<EntryId>) -> Result<()> {
    let mut conn = initialize_database_connection()?;
    let tx = conn.transaction()?;
    for entry_id in entry_ids {
        let entry_info = get_entry_info_with_conn(&tx, entry_id)?;
        // set tags implicitly resolves
        set_tags_with_conn(&tx, entry_info.entry_id(), &entry_info.details().tags)?;
    }
    tx.commit()?;
    Ok(())
}

fn resolve_tags_with_conn(conn: &Connection, tags: &Vec<Tag>) -> Result<Vec<Tag>> {
    fn inner_resolve_tags_with_conn(conn: &Connection, tags: &Vec<Tag>, mut was_aliased_tagstrings: Vec<String>) -> Result<Vec<Tag>> {
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
            for link in &tag_data.links {
                let is_from_tag = link.from_tagstring == tag_data.tag.to_tagstring();
                let to_tag = Tag::from_tagstring(&link.to_tagstring);
                if (link.link_type == TagLinkType::Implication) && is_from_tag {
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
                }
            }
            if !was_aliased(&tag_data.tag.to_tagstring(), &was_aliased_tagstrings) {
                add_tag(&tag_data.tag, &mut resolved_tags, None);
            }
        }
        if !is_resolved {
            resolved_tags = inner_resolve_tags_with_conn(conn, &resolved_tags, was_aliased_tagstrings)?;
        }

        Ok(resolved_tags)
    }
    inner_resolve_tags_with_conn(&conn, tags, vec![])
}
pub fn resolve_tags(tags: &Vec<Tag>) -> Result<Vec<Tag>> {
    let conn = initialize_database_connection()?;
    resolve_tags_with_conn(&conn, tags)
}
// pub fn does_tag

pub fn does_tag_link_exist(link: &TagLink) -> Result<bool> {
    let conn = initialize_database_connection()?;

    let mut statement = conn.prepare("SELECT 1 FROM tag_links WHERE type = ?1 AND from_tag = ?2 AND to_tag = ?3")?;
    let exists = statement.exists(params![link.link_type.to_string(), link.from_tagstring, link.to_tagstring])?;

    Ok(exists)
}

pub fn does_tagstring_exist(tagstring: &String) -> Result<bool> {
    let does_exist = does_tag_exist(&Tag::from_tagstring(tagstring))?;
    Ok(does_exist)
}

pub fn does_tag_exist(tag: &Tag) -> Result<bool> {
    Ok(filter_to_unknown_tags(&vec![tag.clone()])?.len() == 0)
}

pub fn filter_to_unknown_tags(tags: &Vec<Tag>) -> Result<Vec<Tag>> {
    let conn = initialize_database_connection()?;
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

fn set_tags_with_conn(conn: &Connection, entry_id: &EntryId, tags: &Vec<Tag>) -> Result<Vec<Tag>> {
    let resolved_tags = resolve_tags_with_conn(conn, tags)?;
    clear_entry_tags_with_conn(conn, entry_id)?;

    let mut insert_tag_stmt = if entry_id.is_media_entry_id() {
        conn.prepare("INSERT OR IGNORE INTO entry_tags (hash, tag) VALUES (?1, ?2)")?
    } else {
        conn.prepare("INSERT OR IGNORE INTO entry_tags (link_id, tag) VALUES (?1, ?2)")?
    };

    for tag in resolved_tags.iter() {
        match entry_id {
            EntryId::MediaEntry(hash) => {
                insert_tag_stmt.execute(params![hash, tag.to_tagstring()])?;
            }
            EntryId::PoolEntry(link_id) => {
                insert_tag_stmt.execute(params![link_id, tag.to_tagstring()])?;
            }
        }
    }
    Ok(resolved_tags)
}

pub fn set_tags(entry_id: &EntryId, tags: &Vec<Tag>) -> Result<Vec<Tag>> {
    let conn = initialize_database_connection()?;
    set_tags_with_conn(&conn, entry_id, tags)
}

pub fn get_all_hashes() -> Result<Vec<String>> {
    let conn = initialize_database_connection()?;
    let mut statement = conn.prepare("SELECT hash FROM entry_info")?;
    let rows = statement.query_map([], |row| row.get(0))?;
    let mut hashes: Vec<String> = Vec::new();
    for hash_result in rows {
        let mut hash_opt: Option<String> = hash_result?;
        if let Some(hash) = hash_opt.take() {
            hashes.push(hash);
        }
    }
    Ok(hashes)
}
pub fn get_media_links_of_hash(hash: &String) -> Result<Vec<i32>> {
    let conn = initialize_database_connection()?;
    get_media_links_of_hash_with_conn(&conn, hash)
}

pub fn get_media_links_of_hash_with_conn(conn: &Connection, hash: &String) -> Result<Vec<i32>> {
    let mut statement = conn.prepare("SELECT DISTINCT link_id FROM media_links WHERE hash = ?1")?;
    let rows = statement.query_map(params![hash], |row| row.get(0))?;
    let mut link_ids: Vec<i32> = Vec::new();
    for id_result in rows {
        link_ids.push(id_result?);
    }
    Ok(link_ids)
}
pub fn get_hashes_of_media_link(link_id: &i32) -> Result<Vec<String>> {
    let conn = initialize_database_connection()?;
    get_hashes_of_media_link_with_conn(&conn, link_id)
}
pub fn get_hashes_of_media_link_with_conn(conn: &Connection, link_id: &i32) -> Result<Vec<String>> {
    // let conn = initialize_database_connection()?;
    let mut statement = conn.prepare("SELECT hash FROM media_links WHERE link_id = ?1 ORDER BY value ASC")?;
    let rows = statement.query_map(params![link_id], |row| row.get(0))?;
    let mut hashes: Vec<String> = Vec::new();
    for hash_result in rows {
        hashes.push(hash_result?);
    }

    Ok(hashes)
}

pub fn delete_tag(tag: &Tag) -> Result<()> {
    let conn = initialize_database_connection()?;
    delete_tag_with_conn(&conn, tag)
}

fn delete_tag_with_conn(conn: &Connection, tag: &Tag) -> Result<()> {
    let s_tag = tag.someified();
    conn.execute(
        "DELETE FROM tag_info WHERE name = ?1 AND namespace = ?2",
        params![s_tag.name, s_tag.namespace],
    )?;
    conn.execute("DELETE FROM tag_links WHERE from_tag = ?1", params![s_tag.to_tagstring()])?;
    conn.execute("DELETE FROM tag_links WHERE to_tag = ?1", params![s_tag.to_tagstring()])?;
    conn.execute("DELETE FROM entry_tags WHERE tag = ?1", params![s_tag.to_tagstring()])?;

    Ok(())
}

pub fn rekey_database(new_key: &String) -> Result<Option<(PathBuf, PathBuf)>> {
    // https://www.zetetic.net/sqlcipher/sqlcipher-api/#rekey
    let is_key_empty = new_key.is_empty();
    let is_database_unencrypted = is_database_unencrypted()?;
    if is_database_unencrypted ^ is_key_empty {
        // plaintext -> encry OR encry -> plaintext
        let temp_filename = rand::distributions::Alphanumeric.sample_string(&mut rand::thread_rng(), 16);
        let old_db_path = Config::global().path.database()?;
        let mut new_db_path = old_db_path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        new_db_path.push(temp_filename.clone());
        
        let new_db_path_str = format!("file:///{}", new_db_path.to_str().unwrap());
        let conn = open_database_connection()?;
        apply_database_key_to_conn(&conn, &get_database_key())?;
        
        conn.execute("ATTACH DATABASE ?1 AS ?2 KEY ?3", params![new_db_path_str, temp_filename, new_key])?;
        let _: Option<usize> = conn.query_row("SELECT sqlcipher_export(?1)", params![temp_filename], |row| row.get(0))?;
        conn.execute("DETACH DATABASE ?1", params![temp_filename])?;
        conn.close().map_err(|_| anyhow!("failed to close conn"))?;

        // fs::remove_file(&db_path)?;
        // fs::rename(temp_filename, db_path)?; //TODO: recourse when we fail here
        set_db_key(new_key);
        Ok(Some((old_db_path, new_db_path)))
    } else {
        // encry -> encry OR plaintext -> plaintext
        let conn = open_database_connection()?;
        apply_database_key_to_conn(&conn, &get_database_key())?;
        conn.pragma_update(None, "rekey", new_key)?;
        set_db_key(new_key);
        Ok(None)
    }
}

pub fn rename_tag(old_tag: &Tag, new_tag: &Tag) -> Result<()> {
    let mut conn = initialize_database_connection()?;
    let tx = conn.transaction()?;
    let old_tag = old_tag.someified();
    let new_tag = new_tag.someified();
    let old_tagstring = old_tag.to_tagstring();
    let new_tagstring = new_tag.to_tagstring();

    register_tag_with_conn(&tx, &new_tag)?;
    if old_tagstring == new_tagstring {
        Ok(())
    } else {
        let old_tag_data = load_tag_data_with_conn(&tx, &old_tag)?;
        for old_link in old_tag_data.links {
            let mut new_link = old_link.clone();
            if new_link.from_tagstring == old_tagstring {
                new_link.from_tagstring = new_tagstring.clone();
            } else {
                new_link.to_tagstring = new_tagstring.clone();
            }
            register_tag_link_with_conn(&tx, &new_link)?;
        }

        let mut media_stmt = tx.prepare("SELECT hash FROM entry_tags WHERE tag = ?1")?;
        let mut pool_stmt = tx.prepare("SELECT link_id FROM entry_tags WHERE tag = ?1")?;
        let hash_results = media_stmt.query_map(params![old_tagstring], |row| row.get("hash"))?;
        let link_id_results = pool_stmt.query_map(params![old_tagstring], |row| row.get("link_id"))?;

        let mut associated_entries: Vec<EntryId> = Vec::new();

        for hash in hash_results {
            if let Ok(hash) = hash {
                associated_entries.push(EntryId::MediaEntry(hash))
            }
        }
        for link_id in link_id_results {
            if let Ok(link_id) = link_id {
                associated_entries.push(EntryId::PoolEntry(link_id))
            }
        }

        for entry in associated_entries.iter() {
            match entry {
                EntryId::MediaEntry(hash) => {
                    tx.execute("INSERT INTO entry_tags (hash, tag) VALUES (?1, ?2)", params![hash, new_tagstring])?;
                }
                EntryId::PoolEntry(link_id) => {
                    tx.execute("INSERT INTO entry_tags (link_id, tag) VALUES (?1, ?2)", params![link_id, new_tagstring])?;
                }
            }
        }
        drop(media_stmt);
        drop(pool_stmt);
        delete_tag_with_conn(&tx, &old_tag)?;
        tx.commit()?;
        Ok(())
    }
}

pub fn delete_tag_link(link: &TagLink) -> Result<()> {
    let conn = initialize_database_connection()?;
    delete_tag_link_with_conn(&conn, link)
}

fn delete_tag_link_with_conn(conn: &Connection, link: &TagLink) -> Result<()> {
    conn.execute(
        "DELETE FROM tag_links WHERE type = ?1 AND from_tag = ?2 AND to_tag = ?3 ",
        params![link.link_type.to_string(), link.from_tagstring, link.to_tagstring],
    )?;

    Ok(())
}

pub fn register_tag(tag: &Tag) -> Result<()> {
    let conn = initialize_database_connection()?;
    register_tag_with_conn(&conn, tag)
}

fn register_tag_with_conn(conn: &Connection, tag: &Tag) -> Result<()> {
    let s_tag = tag.someified();
    conn.execute(
        "DELETE FROM tag_info WHERE name = ?1 AND namespace = ?2",
        params![s_tag.name, s_tag.namespace],
    )?;
    conn.execute(
        "INSERT INTO tag_info (name, namespace, description)
            VALUES (?1, ?2, ?3)",
        params![
            s_tag.name,
            s_tag.namespace.as_ref().unwrap_or(&"".to_string()),
            s_tag.description.as_ref().unwrap_or(&"".to_string())
        ],
    )?;

    Ok(())
}

pub fn register_tag_link(link: &TagLink) -> Result<()> {
    let conn = initialize_database_connection()?;
    register_tag_link_with_conn(&conn, link)
}

fn register_tag_link_with_conn(conn: &Connection, link: &TagLink) -> Result<()> {
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

pub fn get_all_tag_data() -> Result<Vec<TagData>> {
    let conn = initialize_database_connection()?;
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
    all_tag_data.sort_by(|a, b| b.occurances.cmp(&a.occurances));

    Ok(all_tag_data)
}

fn delegate_to_conn_pool<F>(f: F) -> Result<()>
where
    F: Fn() + Send + 'static + Clone,
{
    (0..DATABASE_WORKERS_PER_TASK)
        .map(|_| thread::spawn(f.clone()))
        .collect::<Vec<_>>()
        .into_iter()
        .map(thread::JoinHandle::join)
        .collect::<Result<(), _>>()
        .map_err(|_| anyhow::Error::msg("db operation failed"))
}

fn open_conn(conn_pool: &Pool<SqliteConnectionManager>) -> PooledConnection<SqliteConnectionManager> {
    conn_pool.get().expect("pool too busy")
}

pub fn load_gallery_entries_with_requests(requests: Vec<CompleteDataRequest>) -> Result<()> {
    puffin::profile_scope!("data_load_gallery_entries");

    let conn_pool = generate_conn_pool()?;
    let requests = Arc::new(Mutex::new(requests));
    delegate_to_conn_pool(move || {
        let conn = open_conn(&conn_pool);
        loop {
            let mut requests = requests.lock();
            if requests.len() > 0 {
                let CompleteDataRequest {
                    info_request,
                    preview_request,
                } = requests.remove(0);
                drop(requests);
                let entry_id = info_request.entry_id;
                let entry_info = get_entry_info_with_conn(&conn, &entry_id);
                let image = load_thumbnail_with_conn(&conn, &entry_id).and_then(|image| ui::generate_retained_image(&image));
                preview_request.sender.send(image.map(|image| MediaPreview::Picture(image)));
                info_request.sender.send(entry_info);
            } else {
                break;
            }
        }
    })
}
// pub fn load_thumbnail_with_requests(requests: Vec<DataRequest<RetainedImage>>) -> Result<()> {
//     let conn_pool = get_conn_pool();
//     let requests = Arc::new(Mutex::new(requests));
//     delegate_to_conn_pool(move || {
//         let conn = open_conn(conn_pool);
//         loop {
//             let mut requests = requests.lock().unwrap();
//             if requests.len() > 0 {
//                 let next_request = requests.remove(0);
//                 drop(requests);
//                 let image = load_thumbnail_with_conn(&conn, &next_request.entry_id).and_then(|image| ui::generate_retained_image(&image));
//                 next_request.sender.send(image);

//             } else {
//                 break;
//             }
//         }
//     })
// }
fn arc_mut<T>(t: T) -> Arc<Mutex<T>> {
    Arc::new(Mutex::new(t))
}
pub fn load_entry_info_with_requests(requests: Vec<DataRequest<EntryInfo>>) -> Result<()> {
    // let conn = initialize_database_connection()?;
    // for request in requests {
    //     let entry_info = load_entry_info_with_conn(&conn, &request.entry_id);
    //     request.sender.send(entry_info);
    // }
    // dbg!(&requests.len());
    let conn_pool = generate_conn_pool()?;
    let requests = Arc::new(Mutex::new(requests));
    delegate_to_conn_pool(move || {
        let conn = open_conn(&conn_pool);
        loop {
            let mut requests = requests.lock();
            if requests.len() > 0 {
                let next_request = requests.remove(0);
                drop(requests);
                let entry_info = get_entry_info_with_conn(&conn, &next_request.entry_id);
                next_request.sender.send(entry_info);
            } else {
                break;
            }
        }
    })
}

impl FallibleSender for Vec<RegistrationForm> {
    fn fail(self, error: anyhow::Error) {
        self.into_iter().for_each(|r| {
            r.importation_result_sender
                .send(ImportationStatus::Fail(anyhow::Error::msg(error.to_string())))
        });
    }
}

pub fn register_media_with_forms(reg_forms: Vec<RegistrationForm>) -> Result<()> {
    let (mut conn, reg_forms) = initialize_database_connection_with_senders(reg_forms)?;
    let trans = conn.transaction()?;

    for reg_form in reg_forms {
        let status = register_media_with_conn(&trans, &reg_form);
        reg_form.importation_result_sender.send(status);
    }

    trans.commit()?;
    Ok(())
}

/* FIXME: why fails?
let reg_forms = arc_mut(reg_forms);
let conn_pool = get_conn_pool();
delegate_to_conn_pool(move || {
    let mut conn = open_conn(conn_pool);
    let tx = conn.transaction().unwrap();
    loop {
        let mut reg_forms = reg_forms.lock().unwrap();
        if reg_forms.len() > 0 {
            let next_reg_form = reg_forms.remove(0);
            drop(reg_forms);
            let status = register_media_with_conn(&tx, &next_reg_form);
            next_reg_form.importation_result_sender.send(status);
        } else {
            break;
        }
    }
})

*/

fn register_media_with_conn(conn: &Connection, reg_form: &RegistrationForm) -> ImportationStatus {
    let register = || -> Result<ImportationStatus> {
        let hasher_config = HasherConfig::new().hash_alg(HashAlg::DoubleGradient);
        let hasher = hasher_config.to_hasher();
        let mut perceptual_hash: Option<String> = None;
        if let Some(mime) = reg_form.mimetype.first() {
            if mime.type_() == mime_guess::mime::IMAGE {
                let image = image::load_from_memory(&reg_form.bytes)?;
                perceptual_hash = Some(hex::encode(hasher.hash_image(&image).as_bytes()));
            } else if mime.type_() == mime_guess::mime::APPLICATION {
            }
        }

        let sha_hash = sha256::digest_bytes(&reg_form.bytes);
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let serialized_mime = reg_form.mimetype.first().map(|mime| mime.to_string());
        // }

        let mut statement = conn.prepare("SELECT 1 FROM entry_info WHERE hash = ?")?;
        let exists = statement.exists(params![sha_hash])?;
        if exists {
            return Ok(ImportationStatus::Duplicate);
        }

        let insert_result = conn.execute(
            "INSERT INTO entry_info (hash, perceptual_hash, mime, date_registered, size, is_independant)
                        VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                sha_hash,
                perceptual_hash,
                serialized_mime,
                timestamp,
                reg_form.bytes.len(),
                reg_form.linking_dir.is_none()
            ],
        );

        match insert_result {
            Ok(_) => {
                conn.execute("INSERT INTO media_bytes (hash, bytes) VALUES (?1, ?2)", params![sha_hash, reg_form.bytes])?;
                if let Some(linking_dir) = &reg_form.linking_dir {
                    let mut dir_link_map = reg_form.dir_link_map.lock();
                    let link_id = if let Some(link_id) = dir_link_map.get(linking_dir) {
                        *link_id
                    } else {
                        let next_id = create_new_link_with_conn(conn)?;
                        dir_link_map.insert(linking_dir.clone(), next_id);
                        next_id
                    };

                    conn.execute(
                        "INSERT INTO media_links (link_id, hash, value)
                            VALUES (?1, ?2, ?3)",
                        params![link_id, sha_hash, reg_form.linking_value],
                    )?;
                }

                // let _ = load_thumbnail_with_conn(conn, &EntryId::MediaEntry(sha_hash));
                return Ok(ImportationStatus::Success);
            }
            Err(error) => {
                if let rusqlite::Error::SqliteFailure(e, _) = error {
                    if e.code == rusqlite::ErrorCode::ConstraintViolation {
                        return Ok(ImportationStatus::Duplicate);
                    }
                }
                return Ok(ImportationStatus::Fail(error.into()));
            }
        }
    };

    match register() {
        Ok(status) => return status,
        Err(error) => return ImportationStatus::Fail(error),
    };
}
fn get_next_link_id_with_conn(conn: &Connection) -> Result<i32> {
    let next_id: i32 = conn.query_row("SELECT IFNULL(MAX(link_id), 0) + 1 FROM media_links ", [], |row| row.get(0))?;
    conn.execute("DELETE FROM entry_info WHERE link_id = ?1", params![next_id])?;
    Ok(next_id)
}
fn time_now() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}
fn create_new_link_with_conn(conn: &Connection) -> Result<i32> {
    let next_id = get_next_link_id_with_conn(conn)?;
    let timestamp = time_now()?;
    conn.execute(
        "INSERT INTO entry_info (link_id, date_registered) VALUES (?1, ?2)",
        params![next_id, timestamp],
    )?;
    Ok(next_id)
}
fn load_tag_data_with_conn(conn: &Connection, tag: &Tag) -> Result<TagData> {
    let mut count_stmt = conn.prepare("SELECT COUNT(*) FROM entry_tags WHERE tag = ?1")?;
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

fn set_media_link_value_with_conn(conn: &Connection, link_id: &i32, hash: &String, value: i64) -> Result<()> {
    conn.execute(
        "UPDATE media_links SET value = ?1 WHERE link_id = ?2 AND hash = ?3",
        params![value, link_id, hash],
    )?;
    Ok(())
}

pub fn delete_cached_thumbnail(entry_id: &EntryId) -> Result<()> {
    let conn = initialize_database_connection()?;
    match entry_id {
        EntryId::MediaEntry(hash) => conn.execute("DELETE FROM thumbnail_cache WHERE hash = ?1", params![hash])?,
        EntryId::PoolEntry(link_id) => conn.execute("DELETE FROM thumbnail_cache WHERE link_id = ?1", params![link_id])?,
    };

    Ok(())
}

pub fn merge_pool_links(link_id_a: &i32, link_id_b: &i32, dest_link_id: &i32) -> Result<()> {
    let mut conn = initialize_database_connection()?;
    let tx = conn.transaction()?;
    let hashes = get_hashes_of_media_link_with_conn(&tx, link_id_a)?
        .into_iter()
        .chain(get_hashes_of_media_link_with_conn(&tx, link_id_b)?)
        .collect::<Vec<String>>();
    let entry_info_a = get_entry_info_with_conn(&tx, &EntryId::PoolEntry(*link_id_a))?;
    let entry_info_b = get_entry_info_with_conn(&tx, &EntryId::PoolEntry(*link_id_b))?;

    let tags = entry_info_a
        .details()
        .tags
        .clone()
        .into_iter()
        .chain(entry_info_b.details().tags.clone())
        .collect::<Vec<Tag>>();

    let (keep_id, delete_id) = if dest_link_id == link_id_a {
        (link_id_a, link_id_b)
    } else if dest_link_id == link_id_b {
        (link_id_b, link_id_a)
    } else {
        unreachable!();
        // delete_entry_with_conn(&tx, entry_info_a.entry_id())?;
        // delete_entry_with_conn(&tx, entry_info_b.entry_id())?;
        // let entry_id = EntryId::PoolEntry(create_pool_link(&hashes)?);
        // set_tags(&entry_id, &tags)?;
    };

    delete_entry_with_conn(&tx, &EntryId::PoolEntry(*delete_id))?;
    add_media_to_link_with_conn(&tx, keep_id, &hashes)?;
    set_tags_with_conn(&tx, &EntryId::PoolEntry(*keep_id), &tags)?;

    tx.commit()?;
    Ok(())
}

pub fn add_media_to_link(link_id: &i32, hashes: &Vec<String>) -> Result<()> {
    let conn = initialize_database_connection()?;
    add_media_to_link_with_conn(&conn, link_id, hashes)
}

fn add_media_to_link_with_conn(conn: &Connection, link_id: &i32, hashes: &Vec<String>) -> Result<()> {
    for hash in hashes {
        let mut statement = conn.prepare("SELECT 1 FROM media_links WHERE link_id = ?1 AND hash = ?2")?;
        if !statement.exists(params![link_id, hash])? {
            conn.execute(
                "INSERT INTO media_links (link_id, value, hash) VALUES (?1, ?2, ?3)",
                params![link_id, None::<&i32>, hash],
            )?;
        }
        set_independance_with_conn(&conn, hash, false)?;
    }
    Ok(())
}

pub fn create_pool_link(hashes: &Vec<String>) -> Result<i32> {
    let mut conn = initialize_database_connection()?;
    let tx = conn.transaction()?;
    let next_id = create_new_link_with_conn(&tx)?;
    add_media_to_link_with_conn(&tx, &next_id, hashes)?;
    // for (index, hash) in hashes.iter().enumerate() {
    //     tx.execute(
    //         "INSERT INTO media_links (link_id, value, hash) VALUES (?1, ?2, ?3)",
    //         params![next_id, index, hash],
    //     )?;
    //     set_independance_with_conn(&tx, hash, false)?;
    // }
    tx.commit()?;
    Ok(next_id)
}

pub fn get_media_mime_with_conn(conn: &Connection, hash: &String) -> Result<Option<String>> {
    let mut statement = conn.prepare("SELECT mime FROM entry_info WHERE hash = ?1")?;
    let mime_type: Option<String> = statement.query_row(params![hash], |row| row.get("mime"))?;
    Ok(mime_type)
}

pub fn export_entry(entry_id: &EntryId, mut export_path: PathBuf) -> Result<PathBuf> {
    let conn = initialize_database_connection()?;
    match entry_id {
        EntryId::PoolEntry(link_id) => todo!(),
        EntryId::MediaEntry(hash) => {
            let bytes = get_media_bytes(hash)?;
            let mime = get_media_mime_with_conn(&conn, hash)?;
            let ext = mime.and_then(|ms| mime_guess::get_mime_extensions_str(ms.as_str()).map(|m| m[0]));
            export_path.push(hash);

            if let Some(ext) = ext {
                export_path.set_extension(ext);
            }
            fs::write(&export_path, bytes)?;
            Ok(export_path)
        }
    }
    // Ok(())
}
