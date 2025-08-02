use crate::util::line_list::LineList;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use tokio::sync::Mutex;
use url::Url;

pub static LNG_MAP: Lazy<Mutex<HashMap<Url, String>>> = Lazy::new(|| Mutex::new(HashMap::new()));

pub static LINE_LIST_MAP: Lazy<Mutex<HashMap<Url, LineList>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
