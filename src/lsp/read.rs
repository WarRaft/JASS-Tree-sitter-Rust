use tokio::io::{AsyncBufReadExt, AsyncReadExt};

pub async fn read<R: AsyncBufReadExt + Unpin>(reader: &mut R) -> Option<String> {
    let mut content_length = 0usize;
    let mut line = String::new();

    loop {
        line.clear();
        if reader.read_line(&mut line).await.ok()? == 0 {
            return None;
        }
        if line == "\r\n" {
            break;
        } else if let Some(cl) = line.strip_prefix("Content-Length:") {
            content_length = cl.trim().parse::<usize>().ok()?;
        }
    }

    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).await.ok()?;
    Some(String::from_utf8(body).ok()?)
}