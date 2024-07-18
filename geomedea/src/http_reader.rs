use crate::feature::Feature;
use crate::io::async_ruszstd::MyRuzstdDecoder;
use crate::packed_r_tree::{Node, PackedRTree, PackedRTreeHttpReader};
use crate::{deserialize_from, serialized_size, Bounds, Header, Result};
use crate::{FeatureLocation, PageHeader};
use bytes::{Bytes, BytesMut};
use futures_util::{Stream, StreamExt};
use std::pin::Pin;
use std::task::{Context, Poll};
use streaming_http_range_client::{HttpClient, HttpRange};

use crate::asyncio::{AsyncRead, AsyncReadExt, BufReader, Take};

#[derive(Debug)]
pub struct HttpReader {
    http_client: HttpClient,
    header: Header,
}

impl HttpReader {
    #[cfg(feature = "writer")]
    pub async fn test_reader(data: &[u8]) -> Result<Self> {
        let http_client = HttpClient::test_client(data);
        Self::new(http_client).await
    }

    pub async fn open(url: &str) -> Result<Self> {
        let http_client = HttpClient::new(url);
        Self::new(http_client).await
    }

    async fn new(mut http_client: HttpClient) -> Result<Self> {
        trace!("starting: opening http reader, reading header");

        // TODO: Figure out how big this should be
        fn estimate_index_size(levels: u32) -> usize {
            let nodes: usize = (0..levels).map(|level| 16usize.pow(level)).sum();
            nodes * Node::serialized_size()
        }

        let overfetch_by = estimate_index_size(3) as u64;
        http_client
            .set_range(0..(Self::header_size() + overfetch_by))
            .await?;
        let mut header_bytes = vec![0u8; Self::header_size() as usize];

        http_client.read_exact(&mut header_bytes).await?;
        let header = deserialize_from(&*header_bytes)?;
        Ok(Self {
            http_client,
            header,
        })
    }

    // TODO: usize?
    fn header_size() -> u64 {
        let header = Header::default();
        serialized_size(&header)
            .expect("calculation of serialization size of default header should succeed")
    }

    pub async fn select_all(&mut self) -> Result<FeatureStream> {
        let mut http_client = self.http_client.split_off();

        let features_count = self.header.feature_count;
        if features_count == 0 {
            warn!("features_count == 0");
        }
        let index_size = PackedRTree::new(features_count).index_size();

        // fast forward over index, and request all the feature data.
        let feature_base = Self::header_size() + index_size;
        debug!("features_count: {features_count:?} index_size: {index_size:?} feature_base: {feature_base:?}");
        http_client
            .seek_to_range(HttpRange::RangeFrom(feature_base..))
            .await?;

        let select_all = SelectAll::new(features_count);
        let stream = Selection::SelectAll(select_all)
            .into_feature_buffer_stream(self.header.is_compressed, http_client)
            .await?;
        Ok(FeatureStream::new(stream))
    }

    pub async fn select_bbox(&mut self, bounds: &Bounds) -> Result<FeatureStream> {
        let http_client = self.http_client.split_off();

        let feature_count = self.header.feature_count;
        if feature_count == 0 {
            warn!("features_count == 0");
        }
        debug!("feature_count: {feature_count:?}");
        let index_starting_offset = Self::header_size();

        let mut index_reader =
            PackedRTreeHttpReader::new(feature_count, http_client, index_starting_offset);
        let feature_locations = index_reader.select_bbox(bounds).await?;
        let feature_start = index_starting_offset + index_reader.tree().index_size();
        let http_client = index_reader.into_http_client();
        debug!("feature_locations: {feature_locations:?}");

        let select_bbox = SelectBbox::new(feature_start, Box::new(feature_locations.into_iter()));
        let stream = Selection::SelectBbox(select_bbox)
            .into_feature_buffer_stream(self.header.is_compressed, http_client)
            .await?;
        Ok(FeatureStream::new(stream))
    }

    pub fn http_client(&self) -> &HttpClient {
        &self.http_client
    }
    pub fn header(&self) -> &Header {
        &self.header
    }
}

struct SelectAll {
    features_left_in_document: u64,
}

struct SelectBbox {
    feature_start: u64,
    feature_locations: Box<dyn Iterator<Item = FeatureLocation>>,
}

impl SelectBbox {
    fn new(
        feature_start: u64,
        feature_locations: Box<dyn Iterator<Item = FeatureLocation>>,
    ) -> Self {
        Self {
            feature_locations,
            feature_start,
        }
    }
}

#[derive(Debug)]
struct AsyncPageReader {
    current_page: Option<CurrentPage>,
    is_compressed: bool,
}

#[derive(Debug)]
struct CurrentPage {
    /// None before current page is set
    page_starting_offset: Option<u64>,
    page_decoder: Box<dyn AsyncPageDecoder>,
}

#[async_trait::async_trait(?Send)]
trait AsyncPageDecoder: std::fmt::Debug + AsyncRead + Unpin {
    fn was_read_to_end(&self) -> bool;
    async fn ff_to_feature_offset(&mut self, offset_within_page: u32) -> Result<()>;
    fn into_inner(self: Box<Self>) -> HttpClient;
}

#[derive(Debug)]
struct ZstdPageDecoder {
    zstd_decoder: Take<MyRuzstdDecoder<BufReader<Take<HttpClient>>>>,
    decoded_page_length: u32,
}

impl ZstdPageDecoder {
    // MJK DEBUG:  decoded_page_length = 0, first time through as part of init
    // MJK DEBUG:  decoded_page_length = 156
    fn new(http_client: Take<HttpClient>, decoded_page_length: u32) -> Self {
        // TODO: implement BufReader for http_client?
        let buffered = BufReader::new(http_client);
        // let zstd_decoder = ZstdDecoder::new(buffered).take(decoded_page_length as u64);
        let zstd_decoder = MyRuzstdDecoder::new(buffered).take(decoded_page_length as u64);
        Self {
            decoded_page_length,
            zstd_decoder,
        }
    }

    fn offset_within_page(&self) -> u32 {
        self.decoded_page_length - self.zstd_decoder.limit() as u32
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl AsyncRead for ZstdPageDecoder {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.zstd_decoder).poll_read(cx, buf)
    }
}

#[cfg(target_arch = "wasm32")]
impl AsyncRead for ZstdPageDecoder {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        debug!("ZstdPageDecoder.poll_read");
        Pin::new(&mut self.zstd_decoder).poll_read(cx, buf)
    }
}

#[async_trait::async_trait(?Send)]
impl AsyncPageDecoder for ZstdPageDecoder {
    async fn ff_to_feature_offset(&mut self, offset_within_page: u32) -> Result<()> {
        assert!(
            offset_within_page >= self.offset_within_page(),
            "shouldn't rewind"
        );
        let distance = offset_within_page - self.offset_within_page();
        let skipped = crate::asyncio::copy(
            &mut (&mut self.zstd_decoder).take(distance as u64),
            &mut crate::asyncio::sink(),
        )
        .await?;
        assert_eq!(skipped, distance as u64);
        Ok(())
    }

    fn was_read_to_end(&self) -> bool {
        self.zstd_decoder.get_ref().get_ref().buffer().is_empty() && self.zstd_decoder.limit() == 0
    }

    fn into_inner(self: Box<Self>) -> HttpClient {
        self.zstd_decoder
            .into_inner()
            .into_inner()
            .into_inner()
            .into_inner()
    }
}

#[derive(Debug)]
struct UncompressedPageDecoder {
    inner: Take<HttpClient>,
    encoded_page_length: u32,
}

impl UncompressedPageDecoder {
    fn new(inner: Take<HttpClient>) -> Self {
        Self {
            encoded_page_length: inner.limit() as u32,
            inner,
        }
    }

    fn offset_within_page(&self) -> u32 {
        self.encoded_page_length - self.inner.limit() as u32
    }
}

#[async_trait::async_trait(?Send)]
impl AsyncPageDecoder for UncompressedPageDecoder {
    async fn ff_to_feature_offset(&mut self, offset_within_page: u32) -> Result<()> {
        assert!(
            offset_within_page >= self.offset_within_page(),
            "shouldn't rewind"
        );
        let distance = offset_within_page - self.offset_within_page();
        let skipped = crate::asyncio::copy(
            &mut (&mut self.inner).take(distance as u64),
            &mut crate::asyncio::sink(),
        )
        .await?;
        assert_eq!(skipped, distance as u64);
        Ok(())
    }

    fn was_read_to_end(&self) -> bool {
        self.inner.limit() == 0
    }

    fn into_inner(self: Box<Self>) -> HttpClient {
        self.inner.into_inner()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl AsyncRead for UncompressedPageDecoder {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

#[cfg(target_arch = "wasm32")]
impl AsyncRead for UncompressedPageDecoder {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

fn new_page_decoder(
    inner: Take<HttpClient>,
    is_compressed: bool,
    decoded_page_length: u32,
) -> Box<dyn AsyncPageDecoder> {
    if is_compressed {
        Box::new(ZstdPageDecoder::new(inner, decoded_page_length))
    } else {
        Box::new(UncompressedPageDecoder::new(inner))
    }
}

impl AsyncPageReader {
    fn new(is_compressed: bool, reader: HttpClient) -> Self {
        // "fake" initial page decoder with an empty reader.
        let page_decoder = new_page_decoder(reader.take(0), is_compressed, 0);

        let current_page = Some(CurrentPage {
            page_starting_offset: None,
            page_decoder,
        });

        Self {
            current_page,
            is_compressed,
        }
    }

    async fn ff_to_location(
        &mut self,
        feature_start: u64,
        location: FeatureLocation,
    ) -> Result<()> {
        // TODO be smarter about this.
        let overfetch = 512_000;

        // First get to the right page.
        let (mut page_decoder, page_starting_offset) = match self
            .current_page
            .take()
            .expect("current_page is always replaced")
        {
            CurrentPage {
                page_decoder,
                page_starting_offset: None,
            } => {
                debug!("first content read - we haven't started any page yet.");
                let mut http_client: HttpClient = page_decoder.into_inner();
                let page_header_start = feature_start + location.page_starting_offset;

                debug!("page_header overfetch: {overfetch:?}");
                let page_header_end = page_header_start + PageHeader::serialized_size() as u64;
                let page_header_range = HttpRange::Range(page_header_start..page_header_end);
                if http_client.contains(&page_header_range) {
                    http_client.seek_to_range(page_header_range).await?;
                } else {
                    let page_header_range =
                        HttpRange::Range(page_header_start..page_header_end + overfetch);
                    http_client.seek_to_range(page_header_range).await?;
                }

                let mut bytes = vec![0; PageHeader::serialized_size()];
                http_client.read_exact(&mut bytes).await?;
                let page_header: PageHeader = deserialize_from(&*bytes)?;

                let page_content_end = page_header_end + page_header.encoded_page_length() as u64;
                let page_content_range = HttpRange::Range(page_header_end..page_content_end);
                if http_client.contains(&page_content_range) {
                    http_client.seek_to_range(page_content_range).await?;
                } else {
                    let page_content_range =
                        HttpRange::Range(page_header_end..page_content_end + overfetch);
                    http_client.seek_to_range(page_content_range).await?;
                }
                (
                    new_page_decoder(
                        http_client.take(page_header.encoded_page_length() as u64),
                        self.is_compressed,
                        page_header.decoded_page_length(),
                    ),
                    location.page_starting_offset,
                )
            }
            CurrentPage {
                page_decoder,
                page_starting_offset: Some(page_starting_offset),
            } if page_starting_offset == location.page_starting_offset => {
                trace!("We've already started reading into the correct page.");
                (page_decoder, page_starting_offset)
            }
            CurrentPage {
                page_decoder,
                page_starting_offset: Some(page_starting_offset),
            } => {
                debug!(
                    "We're currently reading an earlier page, and need to fast forward to the proper page."
                );
                assert!(
                    location.page_starting_offset > page_starting_offset,
                    "Trying to fast forward to page {location:?} from current page with starting offset {page_starting_offset}"
                );
                let mut http_client: HttpClient = page_decoder.into_inner();
                let page_header_start = feature_start + location.page_starting_offset;
                let page_header_end = page_header_start + PageHeader::serialized_size() as u64;

                let page_header_range = HttpRange::Range(page_header_start..page_header_end);
                if http_client.contains(&page_header_range) {
                    http_client.seek_to_range(page_header_range).await?;
                } else {
                    let page_header_range =
                        HttpRange::Range(page_header_start..page_header_end + overfetch);
                    http_client.seek_to_range(page_header_range).await?;
                }

                let mut bytes = vec![0; PageHeader::serialized_size()];
                http_client.read_exact(&mut bytes).await?;
                let page_header: PageHeader = deserialize_from(&*bytes)?;

                let page_content_end = page_header_end + page_header.encoded_page_length() as u64;
                let page_content_range = HttpRange::Range(page_header_end..page_content_end);
                if http_client.contains(&page_content_range) {
                    http_client.seek_to_range(page_content_range).await?;
                } else {
                    let page_content_range =
                        HttpRange::Range(page_header_end..page_content_end + overfetch);
                    http_client.seek_to_range(page_content_range).await?;
                }
                (
                    new_page_decoder(
                        http_client.take(page_header.encoded_page_length() as u64),
                        self.is_compressed,
                        page_header.decoded_page_length(),
                    ),
                    location.page_starting_offset,
                )
            }
        };

        page_decoder
            .ff_to_feature_offset(location.feature_offset)
            .await?;

        self.current_page = Some(CurrentPage {
            page_decoder,
            page_starting_offset: Some(page_starting_offset),
        });
        Ok(())
    }

    fn current_page_was_read_to_end(&self) -> bool {
        let curent_page = self
            .current_page
            .as_ref()
            .expect("always replaced or poisoned");
        curent_page.page_decoder.was_read_to_end()
    }

    async fn next_page(&mut self) -> Result<()> {
        assert!(self.current_page_was_read_to_end());
        let CurrentPage {
            page_starting_offset: _,
            page_decoder,
        } = self.current_page.take().expect("always replaced");

        let mut http_client: HttpClient = page_decoder.into_inner();

        let mut page_header_buffer = vec![0u8; PageHeader::serialized_size()];
        // TODO poison on error
        http_client.read_exact(&mut page_header_buffer).await?;

        let next_page_header: PageHeader = deserialize_from(&*page_header_buffer)?;
        info!("read next PageHeader: {next_page_header:?}");

        // dbg!(&next_page_header);
        let reader = http_client.take(next_page_header.encoded_page_length() as u64);
        let next_page_decoder = new_page_decoder(
            reader,
            self.is_compressed,
            next_page_header.decoded_page_length(),
        );

        self.current_page = Some(CurrentPage {
            page_starting_offset: None, // FIXME: Do I care? Maybe I will for SelectBBox Does this need to be on pageHeader, or I can use CountingReader
            page_decoder: next_page_decoder,
        });
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl AsyncRead for AsyncPageReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let CurrentPage {
            mut page_decoder,
            page_starting_offset,
        } = self
            .current_page
            .take()
            .expect("current_page is always replaced");

        let poll_result = Pin::new(&mut page_decoder).poll_read(cx, buf);

        self.current_page = Some(CurrentPage {
            page_decoder,
            page_starting_offset,
        });

        poll_result
    }
}

#[cfg(target_arch = "wasm32")]
impl AsyncRead for AsyncPageReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        debug!("AsyncPageReader.poll_read");
        let CurrentPage {
            mut page_decoder,
            page_starting_offset,
        } = self
            .current_page
            .take()
            .expect("current_page is always replaced");

        let poll_result = Pin::new(&mut page_decoder).poll_read(cx, buf);

        self.current_page = Some(CurrentPage {
            page_decoder,
            page_starting_offset,
        });

        poll_result
    }
}

impl SelectAll {
    fn new(features_left: u64) -> Self {
        // let mut page_header_buffer = vec![0u8; PageHeader::serialized_size()];
        // http_client.read_exact(&mut page_header_buffer).await?;
        // let page_header: PageHeader = deserialize_from(&*page_header_buffer)?;
        Self {
            features_left_in_document: features_left,
        }
    }
}

enum Selection {
    SelectAll(SelectAll),
    SelectBbox(SelectBbox),
}

impl Selection {
    pub async fn into_feature_buffer_stream(
        mut self,
        is_compressed: bool,
        http_client: HttpClient,
    ) -> Result<impl Stream<Item = Result<Bytes>>> {
        let mut page_reader = AsyncPageReader::new(is_compressed, http_client);
        let stream = async_stream::try_stream! {
            loop {
                match self.next_feature_buffer(&mut page_reader).await? {
                    None => break,
                    Some(feature) => {
                        yield feature
                    }
                }
            }
        };
        Ok(Box::pin(stream))
    }

    async fn next_feature_buffer(
        &mut self,
        page_reader: &mut AsyncPageReader,
    ) -> Result<Option<Bytes>> {
        trace!("");
        match self {
            Selection::SelectAll(select_all) => {
                if select_all.features_left_in_document == 0 {
                    // TODO: restore this assert on wasm32
                    #[cfg(not(target_arch = "wasm32"))]
                    debug_assert!(page_reader.read_u8().await.is_err(), "should be empty");
                    return Ok(None);
                }

                if page_reader.current_page_was_read_to_end() {
                    page_reader.next_page().await?;
                }

                select_all.features_left_in_document -= 1;
            }
            Selection::SelectBbox(select_bbox) => {
                let Some(next_location) = select_bbox.feature_locations.next() else {
                    return Ok(None);
                };

                page_reader
                    .ff_to_location(select_bbox.feature_start, next_location)
                    .await?;
            }
        }

        let mut len_bytes = [0u8; 8];
        page_reader.read_exact(&mut len_bytes).await?;
        let feature_len = u64::from_le_bytes(len_bytes);

        let mut feature_buffer = BytesMut::zeroed(feature_len as usize);
        page_reader.read_exact(&mut feature_buffer).await?;

        Ok(Some(feature_buffer.freeze()))
    }
}

pub struct FeatureStream<'a> {
    inner: Box<dyn Stream<Item = Result<Feature>> + Unpin + 'a>,
}

impl<'a> FeatureStream<'a> {
    fn new(stream: impl Stream<Item = Result<Bytes>> + Unpin + 'a) -> Self {
        let inner = stream.map(move |feature_buffer| {
            let feature = deserialize_from::<_, Feature>(feature_buffer?.as_ref())?;
            // trace!("yielding feature: {feature:?}");
            Ok(feature)
        });
        Self {
            inner: Box::new(inner),
        }
    }
}

impl Stream for FeatureStream<'_> {
    type Item = Result<Feature>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

#[cfg(test)]
#[cfg(feature = "writer")]
mod test {
    use super::*;
    use crate::feature::PropertyValue;
    use crate::{ensure_logging, wkt, Geometry, LngLat};

    #[tokio::test]
    async fn select_all_uncompressed() {
        select_all(false).await
    }

    #[tokio::test]
    async fn select_all_compressed() {
        select_all(true).await
    }

    async fn select_all(is_compressed: bool) {
        ensure_logging();
        let bytes = crate::test_data::small_pages(4, is_compressed);

        let mut reader = HttpReader::test_reader(&bytes).await.unwrap();
        let mut stream = reader.select_all().await.unwrap();

        let first_feature = stream.next().await.unwrap().unwrap();
        let Geometry::Point(point) = first_feature.geometry() else {
            panic!("unexpected geometry");
        };
        let expected = wkt!(POINT(3 3));
        assert_eq!(point, &expected);
        assert_eq!(
            &PropertyValue::String("prop-3".to_string()),
            first_feature.property("name").unwrap()
        );

        let remainder: Vec<_> = stream.collect().await;
        let remainder: Vec<Feature> = remainder.into_iter().collect::<Result<Vec<_>>>().unwrap();
        assert_eq!(remainder.len(), 3);

        let stream = reader.select_all().await.unwrap();
        let remainder: Vec<_> = stream.collect().await;
        let remainder: Vec<Feature> = remainder.into_iter().collect::<Result<Vec<_>>>().unwrap();
        assert_eq!(remainder.len(), 4);
    }

    #[tokio::test]
    async fn bbox_uncompressed() {
        bbox(false).await
    }

    #[tokio::test]
    async fn bbox_compressed() {
        bbox(true).await
    }

    async fn bbox(is_compressed: bool) {
        ensure_logging();

        let bytes = crate::test_data::small_pages(4, is_compressed);

        let mut reader = HttpReader::test_reader(&bytes).await.unwrap();
        let bounds = wkt!(RECT(1 1,3 3));
        let mut stream = reader.select_bbox(&bounds).await.unwrap();

        let next = stream.next().await.unwrap().unwrap();
        assert_eq!(next.geometry(), &wkt!(POINT(3 3)).into());

        let next = stream.next().await.unwrap().unwrap();
        assert_eq!(next.geometry(), &wkt!(POINT(2 2)).into());

        let next = stream.next().await.unwrap().unwrap();
        assert_eq!(next.geometry(), &wkt!(POINT(1 1)).into());

        assert!(stream.next().await.transpose().unwrap().is_none());

        let mut reader = HttpReader::test_reader(&bytes).await.unwrap();
        let stream = reader.select_bbox(&bounds).await.unwrap();
        let remainder: Vec<_> = stream.collect().await;
        let remainder: Vec<Feature> = remainder.into_iter().collect::<Result<Vec<_>>>().unwrap();
        assert_eq!(remainder.len(), 3);
    }

    #[tokio::test]
    async fn bbox_compressed_larger_file() {
        ensure_logging();

        let bytes = std::fs::read("../test_fixtures/USCounties-compressed.geomedea").unwrap();
        let mut reader = HttpReader::test_reader(&bytes).await.unwrap();
        let bounds =
            Bounds::from_corners(&LngLat::degrees(-86.0, 10.0), &LngLat::degrees(-85.0, 40.0));
        let mut features = reader.select_bbox(&bounds).await.unwrap();
        let mut count = 0;
        while let Some(feature) = features.next().await.transpose().unwrap() {
            let Geometry::MultiPolygon(_multi_polygon) = feature.geometry() else {
                panic!("expected MultiPolygon, got {:?}", feature.geometry());
            };
            count += 1;
        }
        assert_eq!(count, 140);
    }
}
