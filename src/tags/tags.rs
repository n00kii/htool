use std::fmt;

use anyhow::Result;
use rusqlite::ToSql;

const TAG_DELIM: &str = "::";
const IMPLICATION_STRING: &str = "implication";
const ALIAS_STRING: &str = "alias"; 
const UNDEFINED_STRING: &str = "undefined"; 

#[derive(Debug, Clone)]
pub struct Tag {
    pub name: String,
    pub namespace: Option<String>,
    pub description: Option<String>,
}

impl PartialEq for Tag {
    fn eq(&self, other: &Self) -> bool {
        let (self_noneified, other_noneified) = (self.noneified(), other.noneified());
        self_noneified.name == other_noneified.name && self_noneified.namespace == other_noneified.namespace
    }
}

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone)]
pub struct TagData {
    pub occurances: i32,
    pub tag: Tag,
    pub links: Vec<TagLink>,
}
// #[derive(Debug, Clone)]
// pub struct Implication {
//     pub from: String,
//     pub to: String,
// }
// #[derive(Debug, Clone)]
// pub struct Alias {
//     pub from: String,
//     pub to: String,
// }

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
        if  clone.namespace.is_none() {
            clone.namespace = Some("".to_string());
        }
        if  clone.description.is_none() {
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
        let space = if let Some(space) = self.namespace.as_ref() { space.clone() } else { "".to_string() };
        format!("{}{}{}", space, if space.is_empty() { "" } else { TAG_DELIM }, self.name)
    }
    pub fn from_tagstring(tagstring: &String) -> Self {
        let tag_parts = tagstring.split(TAG_DELIM).collect::<Vec<_>>();
        match tag_parts[..] {
            [namespace, name] => Tag {
                name: name.to_string(),
                namespace: if namespace.to_string().to_string().is_empty() { None} else { Some(namespace.to_string())},
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
}

impl TagLink {
    pub fn empty_implication() -> Self {
        Self {
            from_tagstring: "".to_string(),
            to_tagstring: "".to_string(),
            link_type: TagLinkType::Implication,
        }
    }
    pub fn empty_alias() -> Self {
        Self {
            from_tagstring: "".to_string(),
            to_tagstring: "".to_string(),
            link_type: TagLinkType::Alias,
        }
    }
}

// impl Default for Alias {
//     fn default() -> Self {
//         Self {
//             from: "".to_string(),
//             to: "".to_string(),
//         }
//     }
// }

// impl Default for Implication {
//     fn default() -> Self {
//         Self {
//             from: "".to_string(),
//             to: "".to_string(),
//         }
//     }
// }
