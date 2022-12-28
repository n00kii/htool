use std::{cell::RefCell, fmt, rc::Rc};

use anyhow::Result;
use egui::{Color32, RichText};
use poll_promise::Promise;

use serde::{Deserialize, Serialize};

use crate::{
    config::{Color32Opt, Config},
    data,
    ui::{widgets::autocomplete::AutocompleteOption, SharedState},
    ui::{self, LayoutJobText},
};

const TAG_DELIM: &str = "::";
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
    pub color: Color32Opt,
}

impl Namespace {
    pub fn empty() -> Self {
        Self {
            name: String::new(),
            color: Color32Opt::none(),
        }
    }
    pub fn color32(&self) -> Color32 {
        self.color.0.unwrap_or(ui::text_color())
    }
    pub fn color_array(&self) -> [f32; 4] {
        self.color32().to_array().map(|u| u as f32 / 255.)
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
            name: String::new(),
            namespace: Some(String::new()),
            description: Some(String::new()),
        }
    }
    pub fn someified(&self) -> Self {
        let mut clone = self.clone();
        if clone.namespace.is_none() {
            clone.namespace = Some(String::new());
        }
        if clone.description.is_none() {
            clone.description = Some(String::new());
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
            String::new()
        };
        format!("{}{}{}", space, if space.is_empty() { "" } else { TAG_DELIM }, self.name)
    }
    pub fn to_rich_text(&self, shared_state: &Rc<SharedState>) -> RichText {
        let mut text = RichText::new(&self.name);
        if let Some(namespace_color) = self.namespace_color(shared_state) {
            text = text.color(namespace_color)
        }
        text
    }
    pub fn to_layout_job_text(&self, shared_state: &Rc<SharedState>) -> LayoutJobText {
        LayoutJobText::new(&self.name).with_color(self.namespace_color(shared_state).unwrap_or(ui::text_color()))
    }
    pub fn namespace_color(&self, shared_state: &Rc<SharedState>) -> Option<Color32> {
        if let Some(namespace) = self.noneified().namespace {
            shared_state.namespace_colors.borrow().get(&namespace).copied()
        } else {
            None
        }
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
        }
    }
    pub fn from_tagstrings(tagstrings: &str) -> Vec<Self> {
        let tagstrings = tagstrings.split_whitespace().collect::<Vec<_>>();
        let tagstrings = tagstrings.iter().map(|tagstring| Tag::from_tagstring(&tagstring.to_string()));
        tagstrings.collect::<Vec<_>>()
    }
}

impl TagLink {
    pub fn empty_implication() -> Self {
        Self {
            from_tagstring: String::new(),
            to_tagstring: String::new(),
            link_type: TagLinkType::Implication,
        }
    }
    pub fn to_layout_job_text(&self, shared_state: &Rc<SharedState>) -> Vec<LayoutJobText> {
        vec![
            format!("({} from ", self.link_type.to_string()).into(),
            Tag::from_tagstring(&self.from_tagstring).to_layout_job_text(shared_state),
            " to ".into(),
            Tag::from_tagstring(&self.to_tagstring).to_layout_job_text(shared_state),
            ")".into(),
        ]
    }
    pub fn empty_alias() -> Self {
        Self {
            from_tagstring: String::new(),
            to_tagstring: String::new(),
            link_type: TagLinkType::Alias,
        }
    }
}
pub type TagDataRef = Rc<RefCell<Promise<Result<Vec<TagData>>>>>;

pub fn reload_tag_data(tag_data_ref: &TagDataRef) {
    let _ = tag_data_ref.replace(load_tag_data());
}

pub fn initialize_tag_data() -> TagDataRef {
    Rc::new(RefCell::new(load_tag_data()))
}

fn load_tag_data() -> Promise<Result<Vec<TagData>>> {
    Promise::spawn_thread("load_tag_data", || data::get_all_tag_data())
}

pub fn generate_autocomplete_options(shared_state: &Rc<SharedState>) -> Option<Vec<AutocompleteOption>> {
    if let Some(Ok(tag_data)) = shared_state.tag_data_ref.borrow().ready() {
        Some(
            tag_data
                .iter()
                .map(|tag_data| AutocompleteOption {
                    label: tag_data.tag.name.clone(),
                    value: tag_data.tag.to_tagstring(),
                    color: tag_data.tag.namespace_color(shared_state),
                    description: tag_data.occurances.to_string(),
                    succeeding_space: true
                })
                .collect::<Vec<_>>(),
        )
    } else {
        None
    }
}
