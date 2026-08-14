use std::io::{self, Read};

#[derive(Debug)]
pub(crate) struct LimitedRead {
    pub(crate) bytes: Vec<u8>,
    pub(crate) truncated: bool,
}

pub(crate) fn read_limited(reader: impl Read, limit: usize) -> io::Result<LimitedRead> {
    let mut bytes = Vec::new();
    reader
        .take(limit.saturating_add(1) as u64)
        .read_to_end(&mut bytes)?;

    let truncated = bytes.len() > limit;
    bytes.truncate(limit);

    Ok(LimitedRead { bytes, truncated })
}
