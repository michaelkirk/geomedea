use crate::bounds::Bounds;
use crate::geometry::Bounded;
use crate::io::CountingWriter;
use crate::packed_r_tree::{Node, PackedRTreeWriter};
use crate::{
    deserialize_from, serialize_into, serialized_size, Feature, FeatureLocation, Header,
    PageHeader, Result,
};
use byteorder::{LittleEndian, WriteBytesExt};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::marker::PhantomData;
use tempfile::tempfile;

mod hilbert;

#[derive(Debug)]
pub struct Writer<W: Write> {
    inner: W,
    feature_tempfile: CountingWriter<BufWriter<File>>,
    feature_entries: Vec<FeatureEntry>,
    extent: Bounds,
    header: Header,
    /// How many bytes before rolling over to a new page, note we don't switch to a new page until
    /// after this limit is hit, so pages will be slightly larger than this size.
    page_size_goal: u64,
}

impl<W: Write> Writer<W> {
    pub fn new(inner: W, is_compressed: bool) -> Result<Self> {
        let header = Header {
            is_compressed,
            page_count: 0,
            feature_count: 0,
        };
        Ok(Self {
            inner,
            feature_tempfile: CountingWriter::new(BufWriter::new(tempfile()?), "feature_tempfile"),
            feature_entries: vec![],
            extent: Bounds::empty(),
            header,
            page_size_goal: 1024 * 64, // REVIEW: what should default limit be?
        })
    }

    pub fn page_size_goal(&self) -> u64 {
        self.page_size_goal
    }

    pub fn set_page_size_goal(&mut self, page_size_goal: u64) {
        self.page_size_goal = page_size_goal;
    }

    pub fn add_feature(&mut self, feature: &Feature) -> Result<()> {
        self.header.feature_count += 1;

        let tmp_offset = self.feature_tempfile.total_bytes_written();
        let bounds = feature.geometry().bounds();
        self.extent.extend(&bounds);
        self.feature_entries
            .push(FeatureEntry { bounds, tmp_offset });
        serialize_into(&mut self.feature_tempfile, feature)?;

        Ok(())
    }

    // TODO: do on drop?
    pub fn finish(mut self) -> Result<W> {
        let mut feature_buffer = self
            .feature_tempfile
            .into_inner()
            .into_inner()
            .map_err(|e| e.into_error())?;
        feature_buffer.rewind()?;
        let mut feature_reader = BufReader::new(feature_buffer);

        self.feature_entries.sort_by(|a, b| {
            // PERF: memoize hilbert on node
            let ha = hilbert::scaled_hilbert(&a.bounds.center(), &self.extent);
            let hb = hilbert::scaled_hilbert(&b.bounds.center(), &self.extent);
            hb.cmp(&ha)
        });

        let mut packed_r_tree = PackedRTreeWriter::new(self.feature_entries.len() as u64)?;
        let (page_headers, page_contents) = {
            if self.header.is_compressed {
                let mut page_writer = FeatureWriter::<_, ZstdPageEncoder<_>>::new(
                    BufWriter::new(tempfile()?),
                    self.page_size_goal,
                );
                page_writer.write_features(
                    self.feature_entries,
                    &mut feature_reader,
                    &mut packed_r_tree,
                )?;
                page_writer.finish()?
            } else {
                let mut page_writer = FeatureWriter::<_, UncompressedPageEncoder<_>>::new(
                    BufWriter::new(tempfile()?),
                    self.page_size_goal,
                );
                page_writer.write_features(
                    self.feature_entries,
                    &mut feature_reader,
                    &mut packed_r_tree,
                )?;
                page_writer.finish()?
            }
        };

        self.header.page_count = page_headers.len() as u64;

        // write file header
        serialize_into(&mut self.inner, &self.header)?;

        // write index
        packed_r_tree.write(&mut self.inner)?;

        // Copy ordered features from tmp location to after the index
        let mut page_contents = page_contents.into_inner().map_err(|r| r.into_error())?;
        page_contents.rewind()?;
        for (page_idx, page_header) in page_headers.iter().enumerate() {
            debug!("serializing page #{page_idx} {page_header:?}");
            serialize_into(&mut self.inner, &page_header)?;
            std::io::copy(
                &mut BufReader::new(
                    (&mut page_contents).take(page_header.encoded_page_length() as u64),
                ),
                &mut self.inner,
            )?;
        }

        self.inner.flush()?;
        Ok(self.inner)
    }
}

trait PageEncoder<W: Write>: Write + Sized {
    fn new(inner: W) -> Result<Self>;
    fn total_bytes_in(&self) -> u64;
    fn total_bytes_out(&self) -> u64;
    fn finish(self) -> Result<CountingWriter<W>>;
}

#[derive(Debug)]
struct FeatureEntry {
    bounds: Bounds,
    tmp_offset: u64,
}

enum CurrentPage<W: Write, PE: PageEncoder<W>> {
    Started { page: Page<W, PE> },
    Unstarted { writer: W, next_page_id: u32 },
}

#[derive(Debug)]
struct Page<W: Write, PE: PageEncoder<W>> {
    bounds: Bounds,
    page_id: u32,
    starting_offset: u64,
    feature_count: u32,
    encoder: PE,
    _marker: PhantomData<W>, // Do I need this?
}

impl<W: Write + Seek, PE: PageEncoder<W>> Page<W, PE> {
    pub fn new(page_id: u32, starting_offset: u64, writer: W) -> Result<Self> {
        let encoder = PE::new(writer)?;
        Ok(Self {
            page_id,
            starting_offset,
            bounds: Bounds::empty(),
            feature_count: 0,
            encoder,
            _marker: PhantomData,
        })
    }

    pub fn extend(&mut self, bounds: &Bounds) {
        self.bounds.extend(bounds)
    }

    fn add_feature(&mut self, feature: &Feature) -> Result<(u64, FeatureLocation)> {
        let feature_location = FeatureLocation {
            page_starting_offset: self.starting_offset,
            feature_offset: self.encoder.total_bytes_in() as u32,
        };
        self.extend(&feature.geometry().bounds());
        self.feature_count += 1;

        let serialized_size = serialized_size(feature)?;
        if let Err(e) = self.encoder.write_u64::<LittleEndian>(serialized_size) {
            todo!("Error while serializing size: {e:?}. Poison state to make it clear the writer is corrupted and not usable from this point");
        }
        if let Err(e) = serialize_into(&mut self.encoder, feature) {
            todo!("Error while serializing: {e:?}. Poison state to make it clear the writer is corrupted and not usable from this point");
        }
        debug!("wrote {feature_location:?} with {feature:?}");

        let page_size = self.encoder.total_bytes_in();
        debug!("page size (uncompressed): {page_size}");

        Ok((page_size, feature_location))
    }

    fn finish(self) -> Result<(PageHeader, CountingWriter<W>)> {
        let decoded_page_length = self.encoder.total_bytes_in() as u32;
        let writer = self.encoder.finish()?;
        let encoded_page_length =
            u32::try_from(writer.total_bytes_written()).expect("page must be less than u32 bytes");
        let header = PageHeader::new(encoded_page_length, decoded_page_length, self.feature_count);
        Ok((header, writer))
    }
}

struct FeatureWriter<W: Write, PE: PageEncoder<W>> {
    current_page: Option<CurrentPage<W, PE>>,
    finished_pages: Vec<PageHeader>,
    next_page_starting_offset: u64,
    page_size_goal: u64,
}

impl<W: Write + Seek, PE: PageEncoder<W>> FeatureWriter<W, PE> {
    fn new(writer: W, page_size_goal: u64) -> Self {
        let current_page = CurrentPage::Unstarted {
            writer,
            next_page_id: 0,
        };
        Self {
            current_page: Some(current_page),
            next_page_starting_offset: 0,
            finished_pages: vec![],
            page_size_goal,
        }
    }

    fn finish(mut self) -> Result<(Vec<PageHeader>, W)> {
        let writer = match self
            .current_page
            .take()
            .expect("we always replace current_page")
        {
            CurrentPage::Started { page } => {
                let (finished_page, writer) = page.finish()?;
                self.finished_pages.push(finished_page);
                writer.into_inner()
            }
            CurrentPage::Unstarted { writer, .. } => writer,
        };

        if self.finished_pages.is_empty() {
            self.finished_pages.push(PageHeader::new(0, 0, 0));
        }
        Ok((self.finished_pages, writer))
    }

    fn write_features<R: Read + Seek>(
        &mut self,
        feature_entries: impl IntoIterator<Item = FeatureEntry>,
        mut feature_reader: R,
        packed_r_tree: &mut PackedRTreeWriter,
    ) -> Result<()> {
        for tmp_feature in feature_entries {
            feature_reader.seek(SeekFrom::Start(tmp_feature.tmp_offset))?;
            let feature: Feature = deserialize_from(&mut feature_reader)?;
            let offset = self.add_feature(&feature)?;
            packed_r_tree.push_leaf(Node::leaf_node(tmp_feature.bounds, offset))?;
        }
        Ok(())
    }

    /// If this method errors, this writer may be left in a corrupt state. You must create a new
    /// writer.
    ///
    /// Returns the location of the feature in the pages
    fn add_feature(&mut self, feature: &Feature) -> Result<FeatureLocation> {
        let mut page = match self
            .current_page
            .take()
            .expect("we always replace page_writer")
        {
            CurrentPage::Started { page } => page,
            CurrentPage::Unstarted {
                writer,
                next_page_id,
            } => {
                let starting_offset = self.next_page_starting_offset;
                Page::new(next_page_id, starting_offset, writer)?
            }
        };

        let (page_size, feature_location) = page.add_feature(feature)?;

        // TODO: move this into CurrentPage?
        let next_page = if page_size > self.page_size_goal {
            let page_id = page.page_id;
            let next_page_id = page.page_id + 1;
            let (page_header, writer) = page.finish()?;
            self.finished_pages.push(page_header);
            // Don't I need to account for the size of the page header here?
            self.next_page_starting_offset +=
                writer.total_bytes_written() + PageHeader::serialized_size() as u64;
            assert_eq!(next_page_id as usize, self.finished_pages.len());

            debug!(
               "Finished page {page_id} with {bytes_written} bytes written to output. Next page will start at {next_offset}",
               bytes_written = writer.total_bytes_written(),
               next_offset = self.next_page_starting_offset
            );

            CurrentPage::Unstarted {
                writer: writer.into_inner(),
                next_page_id,
            }
        } else {
            CurrentPage::Started { page }
        };

        std::mem::swap(&mut self.current_page, &mut Some(next_page));
        Ok(feature_location)
    }
}

struct ZstdPageEncoder<W: Write> {
    counting_zstd_encoder: CountingWriter<zstd::Encoder<'static, CountingWriter<W>>>,
}

impl<W: Write> PageEncoder<W> for ZstdPageEncoder<W> {
    fn new(write: W) -> Result<Self> {
        let counting_writer = CountingWriter::new(write, "ZstdPageEncoder output");
        let counting_zstd_encoder = CountingWriter::new(
            zstd::Encoder::new(counting_writer, 0)?,
            "ZstdPageEncoder input",
        );
        Ok(Self {
            counting_zstd_encoder,
        })
    }

    fn total_bytes_in(&self) -> u64 {
        self.counting_zstd_encoder.total_bytes_written()
    }

    fn total_bytes_out(&self) -> u64 {
        self.counting_zstd_encoder
            .inner()
            .get_ref()
            .total_bytes_written()
    }

    fn finish(self) -> Result<CountingWriter<W>> {
        Ok(self.counting_zstd_encoder.into_inner().finish()?)
    }
}

impl<W: Write> Write for ZstdPageEncoder<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Write::write(&mut self.counting_zstd_encoder, buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Write::flush(&mut self.counting_zstd_encoder)
    }
}

struct UncompressedPageEncoder<W: Write> {
    inner: CountingWriter<W>,
}

impl<W: Write> PageEncoder<W> for UncompressedPageEncoder<W> {
    fn new(inner: W) -> Result<Self> {
        Ok(Self {
            inner: CountingWriter::new(inner, "UncompressedPageEncoder"),
        })
    }

    fn total_bytes_in(&self) -> u64 {
        self.inner.total_bytes_written()
    }

    fn total_bytes_out(&self) -> u64 {
        self.inner.total_bytes_written()
    }

    fn finish(self) -> Result<CountingWriter<W>> {
        Ok(self.inner)
    }
}

impl<W: Write> Write for UncompressedPageEncoder<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Write::write(&mut self.inner, buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Write::flush(&mut self.inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ensure_logging, wkt};

    mod test_sizes {
        use super::*;
        use crate::feature::Properties;

        #[test]
        fn uncompressed_page_offsets() {
            test_page_offsets(false)
        }

        #[test]
        fn compressed_page_offsets() {
            test_page_offsets(true)
        }

        fn test_page_offsets(is_compressed: bool) {
            ensure_logging();
            let multipoint = wkt! { MULTIPOINT(1 2,11 12,-1 -2,-11 -12) };
            let mut output = vec![];
            {
                let mut writer = Writer::new(&mut output, is_compressed).unwrap();
                writer.page_size_goal = 15;
                for point in multipoint.points() {
                    let geometry = point.clone().into();
                    let feature = Feature::new(geometry, Properties::empty());
                    writer.add_feature(&feature).unwrap();
                }
                writer.finish().unwrap();
            }

            if is_compressed {
                assert_eq!(337, output.len());
            } else {
                assert_eq!(317, output.len());
            }
        }
    }
}
