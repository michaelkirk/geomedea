use std::io::Write;

#[derive(Debug)]
pub struct CountingWriter<W> {
    inner: W,
    total_bytes_written: u64,
    debug_name: &'static str,
}

impl<W: Write> CountingWriter<W> {
    pub fn new(inner: W, debug_name: &'static str) -> Self {
        CountingWriter {
            inner,
            total_bytes_written: 0,
            debug_name,
        }
    }

    pub fn total_bytes_written(&self) -> u64 {
        self.total_bytes_written
    }

    #[allow(unused)]
    pub fn inner(&self) -> &W {
        &self.inner
    }

    pub fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let bytes_written = self.inner.write(buf)?;
        trace!(
            "{name} writing {bytes_written} bytes {start}..{finish}: {bytes:?}",
            name = self.debug_name,
            bytes = crate::inspector::ByteFormatter(&buf[0..bytes_written]),
            start = self.total_bytes_written,
            finish = self.total_bytes_written + bytes_written as u64
        );
        self.total_bytes_written += bytes_written as u64;
        Ok(bytes_written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
