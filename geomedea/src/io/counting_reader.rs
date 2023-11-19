use std::io::Read;

pub struct CountingReader<R: Read> {
    inner: R,
    total_bytes_read: u64,
    debug_name: &'static str,
}

impl<R: Read> CountingReader<R> {
    pub fn new(inner: R, debug_name: &'static str) -> Self {
        CountingReader {
            inner,
            total_bytes_read: 0,
            debug_name,
        }
    }

    pub fn total_bytes_read(&self) -> u64 {
        self.total_bytes_read
    }
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let bytes_read = self.inner.read(buf)?;
        trace!(
            "{name} reading {bytes_read} bytes {start}..{finish}: {bytes:?}",
            name = self.debug_name,
            bytes = crate::inspector::ByteFormatter(&buf[0..bytes_read]),
            start = self.total_bytes_read,
            finish = self.total_bytes_read + bytes_read as u64
        );

        self.total_bytes_read += bytes_read as u64;
        Ok(bytes_read)
    }
}
