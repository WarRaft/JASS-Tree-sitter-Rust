use crate::lng::bni::parse::parse;
use crate::util::uri_map::URI_MAP;
use url::Url;

pub async fn open(uri: &Url, text: impl AsRef<[u8]>) {
    let mut map = URI_MAP.lock().await;
    let mut entry = map.entry(uri);

    let line_list = &mut entry.line_list;
    line_list.set_text(&text);

    entry.lng.replace("bni".to_string());

    let text_str = std::str::from_utf8(text.as_ref()).expect("Invalid UTF-8");
    let new_tree = entry
        .parser
        .parse(text_str, None)
        .expect("Failed to parse BNI text");
    entry.tree.replace(new_tree);

    parse(entry);
}
