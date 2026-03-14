use crate::lng::ass::parse::parse;
use crate::lng::ass::uri_map::{PARSER_MAP, TREE_MAP};
use crate::util::file_store::{publish_diagnostics, send_refresh_all};
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::uri_map::LNG_URI_MAP;
use crate::util::uri_lock::uri_lock;
use lapce_xi_rope::Rope;
use std::error::Error;
use tree_sitter::Parser;
use url::Url;

pub async fn open(uri: &Url, text: impl AsRef<[u8]>) -> Result<(), Box<dyn Error + Send + Sync>> {
    {
        uri_lock(uri).await;

        let text = std::str::from_utf8(text.as_ref())?;
        let rope = Rope::from(text);

        ROPE_MAP.insert(uri.clone(), rope);
        LNG_URI_MAP.insert(uri.clone(), "angelscript".to_string());

        let mut parser = PARSER_MAP.entry(uri.clone()).or_insert_with(|| {
            let mut parser = Parser::new();
            parser
                .set_language(&tree_sitter_as::language().into())
                .expect("Failed to set language");
            parser
        });

        let new_tree = parser.parse(text, None).expect("Failed to parse AngelScript text");
        TREE_MAP.insert(uri.clone(), new_tree);
    }

    parse(uri).await?;

    publish_diagnostics(uri).await;
    send_refresh_all().await;

    Ok(())
}

