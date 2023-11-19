use crate::io::CountingReader;
use crate::packed_r_tree::{PackedRTree, PackedRTreeReader};
use crate::writer::{FeatureLocation, PageHeader};
use crate::{deserialize_from, serialized_size, Bounds, Feature, Header, Result};
use std::io::{BufReader, Read, Take};
use std::marker::PhantomData;
use zstd::Decoder as ZstdDecoder;

struct PageReader<'r, R: Read + 'r> {
    // Getting rid of this Option would be nice
    // But it's tricky... when we advance pages we need to move the Reader out of the CurrentPage
    // Maybe there's a way we can mutate CurrentPage instead of moving things out of it.
    current_page: Option<CurrentPage<'r, R>>,
    is_compressed: bool,
}

struct CurrentPage<'r, R: Read> {
    page_starting_offset: u64,
    page_decoder: Box<dyn PageDecoder<'r, R>>,
}

impl<'r, R: Read + 'r> PageReader<'r, R> {
    fn new(reader: R, is_compressed: bool) -> Result<Self> {
        let mut reader = CountingReader::new(reader, "PageReader");

        // PERF: This might be a waste for bbox queries which might not even use the first page
        let header: PageHeader = deserialize_from(&mut reader)?;
        let page_decoder = new_page_decoder(
            reader.take(header.encoded_page_length() as u64),
            is_compressed,
            header.decoded_page_length(),
        )?;

        let current_page = Some(CurrentPage {
            page_starting_offset: 0,
            page_decoder,
        });

        Ok(Self {
            current_page,
            is_compressed,
        })
    }

    fn ff_past_any_header(&mut self) -> Result<()> {
        let CurrentPage {
            page_decoder,
            page_starting_offset,
        } = self
            .current_page
            .take()
            .expect("current_page is always replaced");

        if page_decoder.was_read_to_end() {
            let mut reader = page_decoder.into_inner();
            let page_starting_offset = reader.total_bytes_read();
            let header: PageHeader = deserialize_from(&mut reader)?;
            debug!("opening new page: {header:?}");
            let page_decoder = new_page_decoder(
                reader.take(header.encoded_page_length() as u64),
                self.is_compressed,
                header.decoded_page_length(),
            )?;
            self.current_page = Some(CurrentPage {
                page_decoder,
                page_starting_offset,
            });
        } else {
            self.current_page = Some(CurrentPage {
                page_decoder,
                page_starting_offset,
            });
        }
        Ok(())
    }

    fn ff_to_location(&mut self, location: FeatureLocation) -> Result<()> {
        // First get to the right page.
        let (mut page_decoder, page_starting_offset) = match self
            .current_page
            .take()
            .expect("current_page is always replaced")
        {
            CurrentPage {
                page_decoder,
                page_starting_offset,
            } if page_starting_offset == location.page_starting_offset => {
                trace!("We've already started reading into the correct page.");
                (page_decoder, page_starting_offset)
            }
            CurrentPage {
                page_decoder,
                page_starting_offset,
            } => {
                debug!(
                    "We're currently reading an earlier page, and need to fast forward to the proper page."
                );
                assert!(
                    location.page_starting_offset > page_starting_offset,
                    "Trying to fast forward to page {location:?} from current page with starting offset {page_starting_offset}"
                );
                let reader = page_decoder.into_inner();
                assert!(
                    location.page_starting_offset >= reader.total_bytes_read(),
                    "Trying to rewind to {} from {}",
                    location.page_starting_offset,
                    reader.total_bytes_read()
                );
                let distance = location.page_starting_offset - reader.total_bytes_read();
                let mut reader = {
                    let mut ff = reader.take(distance);
                    std::io::copy(&mut ff, &mut std::io::sink())?;
                    ff.into_inner()
                };
                let header: PageHeader = deserialize_from(&mut reader)?;
                let page_decoder = new_page_decoder(
                    reader.take(header.encoded_page_length() as u64),
                    self.is_compressed,
                    header.decoded_page_length(),
                )?;
                (page_decoder, location.page_starting_offset)
            }
        };

        page_decoder.ff_to_feature_offset(location.feature_offset)?;

        self.current_page = Some(CurrentPage {
            page_decoder,
            page_starting_offset,
        });
        Ok(())
    }
}

impl<'r, R: Read + 'r> Read for PageReader<'r, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let CurrentPage {
            mut page_decoder,
            page_starting_offset,
        } = self
            .current_page
            .take()
            .expect("current_page is always replaced");

        let read_size = Read::read(&mut page_decoder, buf)?;

        self.current_page = Some(CurrentPage {
            page_decoder,
            page_starting_offset,
        });

        Ok(read_size)
    }
}

trait PageDecoder<'r, R: Read + 'r>: Read + 'r {
    fn ff_to_feature_offset(&mut self, offset_within_page: u32) -> Result<()>;
    fn was_read_to_end(&self) -> bool;
    fn into_inner(self: Box<Self>) -> CountingReader<R>;
}

struct ZstdPageDecoder<R: Read> {
    zstd_decoder: Take<ZstdDecoder<'static, BufReader<Take<CountingReader<R>>>>>,
    decoded_page_length: u32,
}

impl<R: Read> ZstdPageDecoder<R> {
    fn new(read: Take<CountingReader<R>>, decoded_page_length: u32) -> Result<Self> {
        // Single frame to not read across pages, which currently are just concatenated frames.
        let zstd_decoder = zstd::Decoder::new(read)?.take(decoded_page_length as u64);
        Ok(Self {
            zstd_decoder,
            decoded_page_length,
        })
    }
    fn offset_within_decoded_page_content(&self) -> u32 {
        self.decoded_page_length - self.zstd_decoder.limit() as u32
    }
}

impl<'r, R: Read + 'r> PageDecoder<'r, R> for ZstdPageDecoder<R> {
    fn ff_to_feature_offset(&mut self, offset_within_page: u32) -> Result<()> {
        assert!(
            self.offset_within_decoded_page_content() <= offset_within_page,
            "Trying to rewind to {offset_within_page:?} which is before current offset: {}",
            self.offset_within_decoded_page_content()
        );
        let distance = offset_within_page - self.offset_within_decoded_page_content();
        let amount_copied = std::io::copy(&mut self.take(distance as u64), &mut std::io::sink())?;
        // TODO: handle error gracefully
        assert_eq!(amount_copied, distance as u64);
        debug!("skipped {distance} bytes to next feature at {offset_within_page}");
        assert_eq!(
            self.offset_within_decoded_page_content(),
            offset_within_page
        );
        Ok(())
    }

    fn was_read_to_end(&self) -> bool {
        self.zstd_decoder.limit() == 0
    }

    fn into_inner(self: Box<Self>) -> CountingReader<R> {
        self.zstd_decoder
            .into_inner()
            .finish()
            .into_inner()
            .into_inner()
    }
}

impl<'r, R: Read + 'r> Read for ZstdPageDecoder<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        Read::read(&mut self.zstd_decoder, buf)
    }
}

struct UncompressedPageDecoder<R: Read> {
    inner: Take<CountingReader<R>>,
    current_feature_offset: u32,
}

impl<R: Read> UncompressedPageDecoder<R> {
    fn new(read: Take<CountingReader<R>>) -> Self {
        Self {
            inner: read,
            current_feature_offset: 0,
        }
    }
}

impl<R: Read> Read for UncompressedPageDecoder<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let distance = Read::read(&mut self.inner, buf)?;
        self.current_feature_offset += distance as u32;
        Ok(distance)
    }
}

impl<'r, R: Read + 'r> PageDecoder<'r, R> for UncompressedPageDecoder<R> {
    fn ff_to_feature_offset(&mut self, offset_within_page: u32) -> Result<()> {
        assert!(
            self.current_feature_offset <= offset_within_page,
            "Requested {offset_within_page:?} which is before current offset: {}",
            self.current_feature_offset
        );
        let distance = offset_within_page - self.current_feature_offset;
        let amount_copied = std::io::copy(&mut self.take(distance as u64), &mut std::io::sink())?;
        // TODO: handle error gracefully
        assert_eq!(amount_copied, distance as u64);
        debug!("skipped {distance} bytes to next feature at {offset_within_page}");
        assert_eq!(self.current_feature_offset, offset_within_page);
        Ok(())
    }

    fn was_read_to_end(&self) -> bool {
        self.inner.limit() == 0
    }

    fn into_inner(self: Box<Self>) -> CountingReader<R> {
        self.inner.into_inner()
    }
}

#[derive(Debug)]
pub struct Reader<'r, R: Read + 'r> {
    inner: R,
    header: Header,
    _marker: &'r PhantomData<()>,
}

#[derive(Debug, Clone)]
pub struct FileInfo {
    header: Header,
    index_size: u64,
}
impl FileInfo {
    pub fn index_size(&self) -> u64 {
        self.index_size
    }
    pub fn header_size(&self) -> Result<u64> {
        serialized_size(&self.header)
    }
}

impl<'r, R: Read + 'r> Reader<'r, R> {
    pub fn new(mut reader: R) -> Result<Self> {
        let header: Header = deserialize_from(&mut reader)?;
        Ok(Self {
            inner: reader,
            header,
            _marker: &PhantomData,
        })
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn info(&self) -> FileInfo {
        FileInfo {
            header: self.header.clone(),
            index_size: PackedRTree::new(self.header.feature_count).index_size(),
        }
    }

    pub fn select_all(self) -> Result<FeatureIter<'r, R>> {
        let reader = {
            let index_size = PackedRTree::new(self.header.feature_count).index_size();
            let mut index_reader = self.inner.take(index_size);
            std::io::copy(&mut index_reader, &mut std::io::sink())?;
            index_reader.into_inner()
        };
        let page_reader = PageReader::new(reader, self.header.is_compressed)?;
        Ok(FeatureIter {
            selection: Selection::All,
            page_reader,
            features_left: self.header.feature_count,
        })
    }

    pub fn select_bbox(self, bounds: &Bounds) -> Result<FeatureIter<'r, R>> {
        let (items, reader) = {
            let index_size = PackedRTree::new(self.header.feature_count).index_size();
            let mut index_reader = self.inner.take(index_size);
            let rtree_reader = PackedRTreeReader::new(self.header.feature_count, &mut index_reader);
            debug!("select_bbox with bounds: {bounds:?}");
            let items = rtree_reader.select_bbox(bounds)?;
            debug!("items: {items:?}");
            // Skip past any remaining index bytes
            std::io::copy(&mut index_reader, &mut std::io::sink())?;
            (items, index_reader.into_inner())
        };
        let page_reader = PageReader::new(reader, self.header.is_compressed)?;
        Ok(FeatureIter {
            selection: Selection::Bbox(Box::new(items.into_iter())),
            page_reader,
            features_left: self.header.feature_count,
        })
    }
}

fn new_page_decoder<'r, R: Read + 'r>(
    inner: Take<CountingReader<R>>,
    is_compressed: bool,
    decoded_page_length: u32,
) -> Result<Box<dyn PageDecoder<'r, R>>> {
    let page_decoder: Box<dyn PageDecoder<'r, R>> = if is_compressed {
        Box::new(ZstdPageDecoder::<R>::new(inner, decoded_page_length)?)
    } else {
        Box::new(UncompressedPageDecoder::new(inner))
    };
    Ok(page_decoder)
}

enum Selection {
    All,
    Bbox(Box<dyn Iterator<Item = FeatureLocation>>),
}

// TODO: can we remove this lifetime?
pub struct FeatureIter<'r, R: Read> {
    page_reader: PageReader<'r, R>,
    selection: Selection,
    features_left: u64,
}

impl<R: Read> FeatureIter<'_, R> {
    pub fn next(&mut self) -> Result<Option<Feature>> {
        if self.features_left == 0 {
            return Ok(None);
        }
        self.features_left -= 1;
        match &mut self.selection {
            Selection::All => {
                self.page_reader.ff_past_any_header()?;
            }
            Selection::Bbox(locations) => {
                let Some(next) = locations.next() else {
                    return Ok(None);
                };
                self.page_reader.ff_to_location(next)?;
            }
        }
        let _feature_size: u64 = deserialize_from(&mut self.page_reader)?;
        // dbg!(_feature_size);
        let feature = deserialize_from(&mut self.page_reader)?;
        debug!("read feature: {feature:?}");
        Ok(Some(feature))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ensure_logging, test_data, wkt, Geometry};

    #[test]
    fn select_all_with_uncompressed_single_page() {
        select_all(false, false);
    }

    #[test]
    fn select_all_with_uncompressed_multiple_pages() {
        select_all(false, true);
    }

    #[test]
    fn select_all_with_compressed_single_page() {
        select_all(true, false);
    }

    #[test]
    fn select_all_with_compressed_multiple_pages() {
        select_all(true, true);
    }

    fn select_all(is_compressed: bool, multiple_pages: bool) {
        ensure_logging();
        let output = if multiple_pages {
            test_data::small_pages(4, is_compressed)
        } else {
            test_data::points(4, is_compressed)
        };

        let reader = Reader::new(output.as_slice()).unwrap();
        let mut feature_iter = reader.select_all().unwrap();

        let mut geometries = vec![];
        while let Some(feature) = feature_iter.next().unwrap() {
            geometries.push(feature.geometry().clone());
        }

        // slightly re-ordered vs. input because of hilbert
        assert_eq!(
            geometries,
            vec![
                Geometry::Point(wkt!(POINT(3 3))),
                Geometry::Point(wkt!(POINT(2 2))),
                Geometry::Point(wkt!(POINT(1 1))),
                Geometry::Point(wkt!(POINT(0 0))),
            ]
        );
    }

    #[test]
    fn bbox_with_uncompressed_single_page() {
        bbox(false, false);
    }

    #[test]
    fn bbox_with_uncompressed_multiple_pages() {
        bbox(false, true);
    }

    #[test]
    fn bbox_with_compressed_single_page() {
        bbox(true, false);
    }

    #[test]
    fn bbox_with_compressed_multiple_pages() {
        bbox(true, true);
    }

    fn bbox(is_compressed: bool, multiple_pages: bool) {
        let output = if multiple_pages {
            test_data::small_pages(4, is_compressed)
        } else {
            test_data::points(4, is_compressed)
        };
        let reader = Reader::new(output.as_slice()).unwrap();

        let bounds = wkt!(RECT(1 1,2 2));
        let mut features = reader.select_bbox(&bounds).unwrap();
        assert_eq!(
            features.next().unwrap().unwrap().geometry(),
            &wkt!(POINT(2 2)).into()
        );
        assert_eq!(
            features.next().unwrap().unwrap().geometry(),
            &wkt!(POINT(1 1)).into()
        );
        assert!(features.next().unwrap().is_none());
    }
}
