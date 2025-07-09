#[cfg(target_arch = "wasm32")]
use async_compression::futures::bufread::Decoder;
#[cfg(not(target_arch = "wasm32"))]
use async_compression::tokio::bufread::Decoder;
use async_compression::{codec::Decode, util::PartialBuffer};
use std::any::type_name;

use ruzstd::decoding::FrameDecoder;
use std::fmt::{Debug, Formatter};

use crate::asyncio::{AsyncRead, BufReader};
use std::pin::Pin;
use std::task::{Context, Poll};

pub type GenericRuzstdDecoder<R> = Decoder<R, MyZstdFrameDecoder>;

pub struct MyRuzstdDecoder<R> {
    inner: GenericRuzstdDecoder<BufReader<R>>,
}

impl<R> Debug for MyRuzstdDecoder<R> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", type_name::<Self>())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<R: AsyncRead + Unpin> AsyncRead for MyRuzstdDecoder<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

#[cfg(target_arch = "wasm32")]
impl<R: AsyncRead + Unpin> AsyncRead for MyRuzstdDecoder<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        debug!("MyRuzstdDecoder::poll_read");
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<R: AsyncRead + Unpin> MyRuzstdDecoder<R> {
    pub fn new(reader: R) -> Self {
        let _: &dyn AsyncRead = &reader;
        let buf_reader = BufReader::new(reader);
        let _: &dyn crate::asyncio::AsyncBufRead = &buf_reader;

        let inner = Decoder::new(buf_reader, MyZstdFrameDecoder::new());
        Self { inner }
    }
    pub fn get_ref(&self) -> &R {
        self.inner.get_ref().get_ref()
    }
    pub fn into_inner(self) -> R {
        self.inner.into_inner().into_inner()
    }
}

pub struct MyZstdFrameDecoder {
    inner: FrameDecoder,
    input_buffer: PartialBuffer<Vec<u8>>,
}

impl MyZstdFrameDecoder {
    pub fn new() -> Self {
        let inner = FrameDecoder::new();
        const MAX_BLOCK_SIZE: usize = 1024 * 129; // 128kb + some slop for headers
        MyZstdFrameDecoder {
            inner,
            input_buffer: PartialBuffer::new(Vec::with_capacity(MAX_BLOCK_SIZE)),
        }
    }
}

impl Decode for MyZstdFrameDecoder {
    fn reinit(&mut self) -> std::io::Result<()> {
        error!("MyZstdFrameDecoder::reinit not expected to be called");
        unimplemented!()
    }

    fn decode(
        &mut self,
        input: &mut PartialBuffer<impl AsRef<[u8]>>,
        output: &mut PartialBuffer<impl AsRef<[u8]> + AsMut<[u8]>>,
    ) -> std::io::Result<bool> {
        self.input_buffer.get_mut().extend(input.unwritten());
        input.advance(input.unwritten().len());
        let (bytes_read, bytes_written) = self
            .inner
            .decode_from_to(self.input_buffer.unwritten(), output.unwritten_mut())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        self.input_buffer.advance(bytes_read);
        output.advance(bytes_written);
        Ok(self.inner.is_finished())
    }

    fn flush(
        &mut self,
        _output: &mut PartialBuffer<impl AsRef<[u8]> + AsMut<[u8]>>,
    ) -> std::io::Result<bool> {
        error!("MyZstdFrameDecoder::flush not expected to be called");
        unimplemented!()
    }

    fn finish(
        &mut self,
        output: &mut PartialBuffer<impl AsRef<[u8]> + AsMut<[u8]>>,
    ) -> std::io::Result<bool> {
        assert!(self.inner.is_finished());
        let bytes_read = self
            .inner
            .collect_to_writer(output.unwritten_mut())
            .unwrap();
        output.advance(bytes_read);
        Ok(self.inner.can_collect() == 0)
    }
}
