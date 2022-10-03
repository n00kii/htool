use std::fmt;

use anyhow::Result;
use egui::{Color32, RichText};
use rusqlite::ToSql;
use serde::{Deserialize, Serialize};

use crate::{config::Config, ui::{LayoutJobText, self}};

const TAG_DELIM: &str = "::";
// const TAG_DELIM: &str = "::";
const IMPLICATION_STRING: &str = "implication";
const ALIAS_STRING: &str = "alias";
const UNDEFINED_STRING: &str = "undefined link";

#[derive(Debug, Clone)]
pub struct Tag {
    pub name: String,
    pub namespace: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]

pub struct Namespace {
    pub name: String,
    pub color: [f32; 3],
}

impl Namespace {
    pub fn empty() -> Self {
        Self {
            name: "".to_string(),
            color: [1., 1., 1.],
        }
    }
    pub fn color32(&self) -> Color32 {
        Color32::from_rgb((self.color[0] * 255.) as u8, (self.color[1] * 255.) as u8, (self.color[2] * 255.) as u8)
    }
}

impl PartialEq for Tag {
    fn eq(&self, other: &Self) -> bool {
        let (self_noneified, other_noneified) = (self.noneified(), other.noneified());
        self_noneified.name == other_noneified.name && self_noneified.namespace == other_noneified.namespace
    }
}

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum TagLinkType {
    Implication,
    Alias,
    Undefined,
}

impl fmt::Display for TagLinkType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TagLinkType::Implication => write!(f, "{IMPLICATION_STRING}"),
            TagLinkType::Alias => write!(f, "{ALIAS_STRING}"),
            TagLinkType::Undefined => write!(f, "{UNDEFINED_STRING}"),
        }
    }
}

impl From<String> for TagLinkType {
    fn from(s: String) -> Self {
        match s.as_str() {
            IMPLICATION_STRING => TagLinkType::Implication,
            ALIAS_STRING => TagLinkType::Alias,
            _ => TagLinkType::Undefined,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TagLink {
    pub from_tagstring: String,
    pub to_tagstring: String,
    pub link_type: TagLinkType,
}

impl fmt::Display for TagLink {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "({} from {} to {})",
            self.link_type.to_string(),
            self.from_tagstring,
            self.to_tagstring
        )
    }
}

#[derive(Debug, Clone)]
pub struct TagData {
    pub occurances: i32,
    pub tag: Tag,
    pub links: Vec<TagLink>,
}

impl Tag {
    pub fn empty() -> Self {
        Self {
            name: "".to_string(),
            namespace: Some("".to_string()),
            description: Some("".to_string()),
        }
    }
    pub fn someified(&self) -> Self {
        let mut clone = self.clone();
        if clone.namespace.is_none() {
            clone.namespace = Some("".to_string());
        }
        if clone.description.is_none() {
            clone.description = Some("".to_string());
        }
        clone
    }
    pub fn noneified(&self) -> Self {
        let mut clone = self.clone();
        if let Some(space) = clone.namespace.as_ref() {
            if space.is_empty() {
                clone.namespace = None;
            }
        }
        if let Some(desc) = clone.description.as_ref() {
            if desc.is_empty() {
                clone.description = None;
            }
        }
        clone
    }
    pub fn new(name: String, namespace: Option<String>, description: Option<String>) -> Self {
        Self {
            name,
            namespace,
            description,
        }
    }
    pub fn to_tagstring(&self) -> String {
        let space = if let Some(space) = self.namespace.as_ref() {
            space.clone()
        } else {
            "".to_string()
        };
        format!("{}{}{}", space, if space.is_empty() { "" } else { TAG_DELIM }, self.name)
    }
    pub fn to_rich_text(&self) -> RichText {
        let mut text = RichText::new(&self.name);
        if let Some(namespace_color) = self.namespace_color() {
            text = text.color(namespace_color)
        }
        text
    }
    pub fn to_layout_job_text(&self) -> LayoutJobText {
        LayoutJobText::new(&self.name).with_color(self.namespace_color().unwrap_or(ui::constants::DEFAULT_TEXT_COLOR))
    }
    pub fn namespace_color(&self) -> Option<Color32> {
        if let Some(namespace) = self.noneified().namespace {
            let config = Config::global();
            for c_namespace in config.namespaces.iter() {
                if c_namespace.name == namespace {
                    return Some(c_namespace.color32());
                }
            }
        }
        None
    }
    pub fn from_tagstring(tagstring: &String) -> Self {
        let tag_parts = tagstring.split(TAG_DELIM).collect::<Vec<_>>();
        match tag_parts[..] {
            [namespace, name] => Tag {
                name: name.to_string(),
                namespace: if namespace.to_string().to_string().is_empty() {
                    None
                } else {
                    Some(namespace.to_string())
                },
                description: None,
            },
            _ => Tag {
                name: tagstring.clone(),
                namespace: None,
                description: None,
            },
            // _ => Err(anyhow::Error::msg("invalid tagstring")),
        }
    }
    pub fn from_tagstrings(tagstrings: &String) -> Vec<Self> {
        let tagstrings = tagstrings.split_whitespace().collect::<Vec<_>>();
        let tagstrings = tagstrings.iter().map(|tagstring| Tag::from_tagstring(&tagstring.to_string()));
        tagstrings.collect::<Vec<_>>()
    }
}

impl TagLink {
    pub fn empty_implication() -> Self {
        Self {
            from_tagstring: "".to_string(),
            to_tagstring: "".to_string(),
            link_type: TagLinkType::Implication,
        }
    }
    pub fn to_layout_job_text(&self) -> Vec<LayoutJobText> {
        vec![
            format!("({} from ", self.link_type.to_string()).into(),
            Tag::from_tagstring(&self.from_tagstring).to_layout_job_text(),
            " to ".into(),
            Tag::from_tagstring(&self.to_tagstring).to_layout_job_text(),
            ")".into()
        ]
    }
    pub fn empty_alias() -> Self {
        Self {
            from_tagstring: "".to_string(),
            to_tagstring: "".to_string(),
            link_type: TagLinkType::Alias,
        }
    }
}