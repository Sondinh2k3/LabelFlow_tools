use std::{
    collections::{HashMap, VecDeque},
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
    sync::Mutex,
};
type Entries = (HashMap<String, (PathBuf, String)>, VecDeque<String>);
#[derive(Default)]
pub struct Resources {
    entries: Mutex<Entries>,
}
impl Resources {
    pub fn register(&self, path: PathBuf, mime: &str) -> String {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let existing = entries
            .0
            .iter()
            .find(|(_, value)| value.0 == path)
            .map(|(token, _)| token.clone());
        let token = existing.unwrap_or_else(|| {
            let token = uuid::Uuid::new_v4().to_string();
            entries.0.insert(token.clone(), (path, mime.into()));
            entries.1.push_back(token.clone());
            loop {
                let videos = entries
                    .0
                    .values()
                    .filter(|(_, mime)| mime.starts_with("video/"))
                    .count();
                let over_videos = videos > 8;
                let over_images = entries.0.len() - videos > 64;
                if !over_videos && !over_images {
                    break;
                }
                let position = entries
                    .1
                    .iter()
                    .position(|token| {
                        entries
                            .0
                            .get(token)
                            .is_some_and(|(_, mime)| mime.starts_with("video/") == over_videos)
                    })
                    .unwrap();
                if let Some(old) = entries.1.remove(position) {
                    entries.0.remove(&old);
                }
            }
            token
        });
        if cfg!(windows) {
            format!("http://lfmedia.localhost/{token}")
        } else {
            format!("lfmedia://localhost/{token}")
        }
    }
    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.0.clear();
            entries.1.clear();
        }
    }
    pub fn response(
        &self,
        request: tauri::http::Request<Vec<u8>>,
    ) -> tauri::http::Response<Vec<u8>> {
        use tauri::http::Response;
        let response = (|| -> std::io::Result<_> {
            let token = request.uri().path().trim_start_matches('/');
            let (path, mime) = self
                .entries
                .lock()
                .map_err(|_| std::io::Error::other("Resource registry unavailable"))?
                .0
                .get(token)
                .cloned()
                .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
            let mut file = File::open(path)?;
            let length = file.metadata()?.len();
            let range = request.headers().get("range").and_then(|h| h.to_str().ok());
            let (start, end, partial) = if let Some(range) = range {
                let (start, end) = range
                    .strip_prefix("bytes=")
                    .and_then(|v| v.split_once('-'))
                    .ok_or_else(|| std::io::Error::other("Invalid range"))?;
                let start = start.parse::<u64>().map_err(std::io::Error::other)?;
                let end = if end.is_empty() {
                    length.saturating_sub(1)
                } else {
                    end.parse::<u64>().map_err(std::io::Error::other)?
                };
                if start >= length || end < start {
                    return Err(std::io::Error::other("Unsatisfiable range"));
                }
                (
                    start,
                    end.min(length - 1).min(start + 8 * 1024 * 1024 - 1),
                    true,
                )
            } else {
                if length > 128 * 1024 * 1024 {
                    return Err(std::io::Error::other("Byte range required"));
                }
                (0, length.saturating_sub(1), false)
            };
            let count = if length == 0 { 0 } else { end - start + 1 };
            file.seek(SeekFrom::Start(start))?;
            let mut bytes = Vec::with_capacity(count as usize);
            file.take(count).read_to_end(&mut bytes)?;
            let mut builder = Response::builder()
                .status(if partial { 206 } else { 200 })
                .header("Content-Type", mime)
                .header("Content-Length", bytes.len())
                .header("Accept-Ranges", "bytes")
                .header("Access-Control-Allow-Origin", "*")
                .header("Cache-Control", "private, max-age=300");
            if partial {
                builder = builder.header("Content-Range", format!("bytes {start}-{end}/{length}"));
            }
            Ok(builder.body(bytes).unwrap())
        })();
        response.unwrap_or_else(|error| {
            Response::builder()
                .status(if error.kind() == std::io::ErrorKind::NotFound {
                    404
                } else {
                    416
                })
                .header("Access-Control-Allow-Origin", "*")
                .body(Vec::new())
                .unwrap()
        })
    }
}
