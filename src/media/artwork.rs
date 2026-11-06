use super::MediaError;
use std::fmt;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

const HTTPS_SCHEME: &str = "https://";
const FILE_SCHEME: &str = "file://";
const LOCAL_ROOT: char = '/';
const PERCENT: u8 = b'%';
const HEX_RADIX: u32 = 16;

#[must_use]
pub fn content_key(value: impl Hash) -> String {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct HttpsUrl(String);

impl HttpsUrl {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for HttpsUrl {
    type Err = MediaError;

    fn from_str(url: &str) -> Result<Self, Self::Err> {
        if url.starts_with(HTTPS_SCHEME) {
            Ok(Self(url.to_owned()))
        } else {
            Err(MediaError::InvalidValue)
        }
    }
}

#[derive(Clone)]
pub struct ArtworkBytes {
    pub key: String,
    pub data: Arc<[u8]>,
}

impl PartialEq for ArtworkBytes {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Eq for ArtworkBytes {}

impl fmt::Debug for ArtworkBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ArtworkBytes")
            .field("key", &self.key)
            .field("len", &self.data.len())
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Artwork {
    Url(HttpsUrl),
    File(PathBuf),
    Bytes(ArtworkBytes),
}

impl Artwork {
    #[must_use]
    pub fn from_url(url: &str) -> Option<Self> {
        if let Ok(url) = url.parse() {
            return Some(Self::Url(url));
        }
        let path = url.strip_prefix(FILE_SCHEME)?;
        path.starts_with(LOCAL_ROOT)
            .then(|| Self::File(PathBuf::from(percent_decode(path))))
    }

    #[must_use]
    pub fn from_bytes(data: impl Into<Arc<[u8]>>) -> Option<Self> {
        let data = data.into();
        if data.is_empty() {
            return None;
        }
        Some(Self::Bytes(ArtworkBytes {
            key: content_key(&data),
            data,
        }))
    }
}

fn percent_decode(text: &str) -> String {
    let mut decoded = Vec::with_capacity(text.len());
    let mut rest = text.as_bytes();
    while let Some((&byte, tail)) = rest.split_first() {
        if let (PERCENT, [high, low, after @ ..]) = (byte, tail) {
            if let Some(escaped) = hex_pair(*high, *low) {
                decoded.push(escaped);
                rest = after;
                continue;
            }
        }
        decoded.push(byte);
        rest = tail;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_pair(high: u8, low: u8) -> Option<u8> {
    let digit = |byte: u8| char::from(byte).to_digit(HEX_RADIX);
    u8::try_from(digit(high)? * HEX_RADIX + digit(low)?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn art_urls_become_remote_or_local_artwork() {
        assert_eq!(
            Artwork::from_url("https://i.scdn.co/image/x"),
            "https://i.scdn.co/image/x".parse().ok().map(Artwork::Url)
        );
        assert_eq!(
            Artwork::from_url("file:///tmp/.org.chromium.Chromium.Ab%20C"),
            Some(Artwork::File(PathBuf::from(
                "/tmp/.org.chromium.Chromium.Ab C"
            )))
        );
        assert_eq!(Artwork::from_url("http://example.com/a.jpg"), None);
        assert_eq!(Artwork::from_url("file://host/a.jpg"), None);
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz%41"), "%zzA");
    }

    #[test]
    fn only_https_urls_are_remote_artwork() {
        assert!("https://example.com/a.jpg".parse::<HttpsUrl>().is_ok());
        assert_eq!(
            "http://example.com/a.jpg".parse::<HttpsUrl>(),
            Err(MediaError::InvalidValue)
        );
    }

    #[test]
    fn artwork_bytes_are_keyed_by_content() {
        let first = Artwork::from_bytes(vec![1, 2, 3]);
        assert!(first.is_some());
        assert_eq!(first, Artwork::from_bytes(vec![1, 2, 3]));
        assert_ne!(first, Artwork::from_bytes(vec![1, 2, 4]));
        assert_eq!(Artwork::from_bytes(Vec::new()), None);
    }
}
