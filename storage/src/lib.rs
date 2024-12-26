mod indexes;
mod docbuf;

use std::collections::HashMap;
use bytes::BytesMut;

struct MemoryBuf {
    indexes: BytesMut,
    index_count: usize,
    index: HashMap<&'static str, BytesMut>,
    docs: BytesMut,
}

impl MemoryBuf {
    // fn from_file(path: &str) -> MemoryBuf {
    // }
}
