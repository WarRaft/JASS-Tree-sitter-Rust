use crate::http::range::Range;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone)]
pub struct ColorInformation {
    pub range: Range,
    pub color: Color,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Color {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub alpha: f64,
}

#[derive(Debug, Deserialize)]
pub struct ColorPresentationParams {
    pub uri: Url,
    pub color: Color,
    pub range: Range,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorPresentation {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_edit: Option<TextEdit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_text_edits: Option<Vec<TextEdit>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    pub range: Range,
    pub new_text: String,
}

