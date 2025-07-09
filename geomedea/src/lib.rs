#[macro_use]
extern crate log;

mod bounds;
mod error;
mod feature;
mod geometry;
mod http_reader;

use bincode::config::{Configuration, Fixint, LittleEndian};
use bincode::enc::write::SizeWriter;
pub use http_reader::{FeatureStream, HttpReader};
mod format;
pub mod inspector;
pub(crate) mod io;
mod packed_r_tree;
mod reader;
#[cfg(test)]
#[cfg(feature = "writer")]
mod test_data;
mod wkt;

#[cfg(feature = "writer")]
mod writer;

pub use bounds::Bounds;
pub use error::{Error, Result};
use format::{FeatureLocation, PageHeader};
pub use geometry::{
    Geometry, GeometryCollection, LineString, LngLat, MultiLineString, MultiPoint, MultiPolygon,
    Point, Polygon,
};
pub use reader::{FeatureIter, Reader};
#[cfg(feature = "writer")]
pub use writer::Writer;

#[cfg(target_arch = "wasm32")]
use futures_util::io as asyncio;
#[cfg(not(target_arch = "wasm32"))]
use tokio::io as asyncio;

pub use crate::feature::{Feature, Properties, PropertyValue};
use serde::{Deserialize, Serialize};

// How large should we make each page of feature data
// before starting a new page.
pub(crate) const DEFAULT_PAGE_SIZE_GOAL: u64 = 1024 * 64;

// The new `standard()` encoding enables variable int encoding, so we stick with "legacy".
//
// Firstly, varints are unacceptable index nodes, which need to be a fixed size so we can
// randomly seek to specific nodes without deserializing the entire index.
// Secondly, though we could in theory use varint encoding for feature data, and it is significantly
// space saving for uncompressed data, it's a much more modest space improvement when applying
// generic (zstd) compression.
static BINCODE_CONFIG: Configuration<LittleEndian, Fixint> = bincode::config::legacy();

pub(crate) fn serialized_size<T>(value: &T) -> Result<u64>
where
    T: serde::Serialize + ?Sized,
{
    let mut size_writer = SizeWriter::default();
    bincode::serde::encode_into_writer(value, &mut size_writer, BINCODE_CONFIG)?;
    Ok(size_writer
        .bytes_written
        .try_into()
        .expect("non-negative total size"))
}

#[cfg(feature = "writer")]
pub(crate) fn serialize_into<W, T>(mut writer: W, value: &T) -> Result<()>
where
    W: std::io::Write,
    T: serde::Serialize + ?Sized,
{
    let _write_len = bincode::serde::encode_into_std_write(value, &mut writer, BINCODE_CONFIG)?;
    Ok(())
}

pub fn deserialize_from<R, T>(mut reader: R) -> Result<T>
where
    R: std::io::Read,
    T: serde::de::DeserializeOwned,
{
    Ok(bincode::serde::decode_from_std_read(
        &mut reader,
        BINCODE_CONFIG,
    )?)
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Header {
    is_compressed: bool,
    // REVIEW: do we need page count?
    page_count: u64,
    feature_count: u64,
}

#[cfg(test)]
fn ensure_logging() {
    use std::io::Write;

    let debug = true;
    let result = if debug {
        env_logger::builder()
            .format(|buf, record| {
                let file = record.file().unwrap_or("?");
                let line = record
                    .line()
                    .map(|line| line.to_string())
                    .unwrap_or("?".to_string());
                let file_location = format!("{file}:{line:3}");
                let module = record.module_path().unwrap_or("?");
                writeln!(
                    buf,
                    "[ {log_level} {module} {file_location} ] {args}",
                    log_level = record.level(),
                    args = record.args()
                )
            })
            .try_init()
    } else {
        env_logger::try_init()
    };
    if let Err(e) = result {
        eprintln!("Error setting up logging: {e:?}")
    }
}

#[cfg(feature = "writer")]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature::Properties;

    #[test]
    fn empty_uncompressed() {
        empty(false)
    }
    #[test]
    fn empty_compressed() {
        empty(true)
    }

    fn empty(is_compressed: bool) {
        let mut output = vec![];
        {
            let writer = Writer::new(&mut output, is_compressed).unwrap();
            writer.finish().unwrap();
        }
        assert_eq!(output.len(), 29);

        let reader = Reader::new(output.as_slice()).unwrap();
        assert!(reader.select_all().unwrap().try_next().unwrap().is_none());
    }

    #[test]
    fn roundtrip() {
        let mut output = vec![];
        {
            let mut writer = Writer::new(&mut output, false).unwrap();
            let geometry = Geometry::from(wkt! { POINT(1 2) });

            let feature = Feature::new(geometry, Properties::empty());
            writer.add_feature(&feature).unwrap();
            writer.finish().unwrap();
        }

        let reader = Reader::new(output.as_slice()).unwrap();
        let mut features = reader.select_all().unwrap();
        let feature = features.try_next().unwrap().unwrap();
        assert_eq!(
            feature.geometry(),
            &Geometry::Point(LngLat::degrees(1.0, 2.0))
        );
        assert!(features.try_next().unwrap().is_none());
    }

    #[test]
    fn serialize_header() {
        let header = Header {
            is_compressed: false,
            page_count: 1,
            feature_count: 3,
        };
        let mut output = vec![];
        serialize_into(&mut output, &header).unwrap();
        let expected: &[u8] = &[
            0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00,
        ];
        assert_eq!(expected, &output);
    }
}
