use futures_util::StreamExt;
use geomedea::{Bounds, FeatureStream};
use geomedea::{Feature, Geometry, LineString, Polygon, Result};
use geozero::error::{GeozeroError, Result as GeozeroResult};
use geozero::{FeatureProcessor, GeozeroDatasource};
use std::io::Read;

#[derive(Debug)]
pub struct GeozeroReader<'r, R: Read>(geomedea::Reader<'r, R>);

/// Read geomedea into geozero, e.g. for converting geomedea to a different format.
impl<'r, R: Read> GeozeroReader<'r, R> {
    pub fn new(reader: R) -> Result<Self> {
        let inner = geomedea::Reader::new(reader)?;
        Ok(Self(inner))
    }

    pub fn select_all(self) -> Result<GeozeroFeatureIter<'r, R>> {
        let inner = self.0.select_all()?;
        Ok(GeozeroFeatureIter(inner))
    }

    pub fn select_bbox(self, bounds: &Bounds) -> Result<GeozeroFeatureIter<'r, R>> {
        let inner = self.0.select_bbox(bounds)?;
        Ok(GeozeroFeatureIter(inner))
    }
}

pub struct GeozeroFeatureIter<'r, R: Read>(geomedea::FeatureIter<'r, R>);

impl<R: Read> GeozeroDatasource for GeozeroFeatureIter<'_, R> {
    fn process<P: FeatureProcessor>(&mut self, processor: &mut P) -> geozero::error::Result<()> {
        processor.dataset_begin(None)?;
        let mut feature_idx = 0;
        while let Some(feature) = self
            .0
            .try_next()
            .map_err(|e| GeozeroError::Feature(e.to_string()))?
        {
            processing::process_feature(processor, feature_idx, feature)?;
            feature_idx += 1;
        }
        processor.dataset_end()?;
        Ok(())
    }
}

mod processing {
    use super::*;
    use crate::geomedea_to_geozero_column_value;

    pub(crate) fn process_feature<P: FeatureProcessor>(
        processor: &mut P,
        feature_idx: i32,
        feature: Feature,
    ) -> geozero::error::Result<()> {
        processor.feature_begin(feature_idx as u64)?;
        processor.geometry_begin()?;
        process_geometry(processor, feature.geometry(), 0)?;
        processor.geometry_end()?;
        if !feature.properties().is_empty() {
            processor.properties_begin()?;
            for (property_idx, (key, value)) in feature.properties().iter().enumerate() {
                processor.property(property_idx, key, &geomedea_to_geozero_column_value(value))?;
            }
            processor.properties_end()?;
        }
        processor.feature_end(feature_idx as u64)?;
        Ok(())
    }

    fn process_geometry<P: FeatureProcessor>(
        processor: &mut P,
        geometry: &Geometry,
        geometry_idx: usize,
    ) -> geozero::error::Result<()> {
        match geometry {
            Geometry::Point(lnglat) => {
                processor.point_begin(geometry_idx)?;
                processor.xy(lnglat.lng_degrees(), lnglat.lat_degrees(), 0)?;
                processor.point_end(geometry_idx)?;
            }
            Geometry::LineString(line_string) => {
                process_line_string(processor, true, geometry_idx, line_string)?;
            }
            Geometry::Polygon(polygon) => {
                process_polygon(processor, true, geometry_idx, polygon)?;
            }
            Geometry::MultiPoint(multi_point) => {
                processor.multipoint_begin(multi_point.points().len(), geometry_idx)?;
                for point in multi_point.points().iter() {
                    // Apparently calling point_begin for each point breaks GeoJsonWriter
                    // That might be a bug - It seems like point_begin should take "tagged" like
                    // all other multi/element types
                    //processor.point_begin(point_idx)?;
                    processor.xy(point.lng_degrees(), point.lat_degrees(), 0)?;
                    //processor.point_end(point_idx)?;
                }
                processor.multipoint_end(geometry_idx)?;
            }
            Geometry::MultiLineString(multi_line_string) => {
                processor
                    .multilinestring_begin(multi_line_string.line_strings().len(), geometry_idx)?;
                for (line_string_idx, line_string) in
                    multi_line_string.line_strings().iter().enumerate()
                {
                    process_line_string(processor, false, line_string_idx, line_string)?;
                }
                processor.multilinestring_end(geometry_idx)?;
            }
            Geometry::MultiPolygon(multi_polygon) => {
                processor.multipolygon_begin(multi_polygon.polygons().len(), geometry_idx)?;
                for (polygon_idx, polygon) in multi_polygon.polygons().iter().enumerate() {
                    process_polygon(processor, false, polygon_idx, polygon)?;
                }
                processor.multipolygon_end(geometry_idx)?;
            }
            Geometry::GeometryCollection(geometry_collection) => {
                processor.geometrycollection_begin(
                    geometry_collection.geometries().len(),
                    geometry_idx,
                )?;
                for (child_idx, geometry) in geometry_collection.geometries().iter().enumerate() {
                    process_geometry(processor, geometry, child_idx)?;
                }
                processor.geometrycollection_end(geometry_idx)?;
            }
        }
        Ok(())
    }

    fn process_line_string<P: FeatureProcessor>(
        processor: &mut P,
        tagged: bool,
        line_string_idx: usize,
        line_string: &LineString,
    ) -> GeozeroResult<()> {
        processor.linestring_begin(tagged, line_string.points_len(), line_string_idx)?;
        for (coord_idx, coord) in line_string.points().iter().enumerate() {
            processor.xy(coord.lng_degrees(), coord.lat_degrees(), coord_idx)?;
        }
        processor.linestring_end(tagged, line_string_idx)?;
        Ok(())
    }

    fn process_polygon<P: FeatureProcessor>(
        processor: &mut P,
        tagged: bool,
        polygon_idx: usize,
        polygon: &Polygon,
    ) -> GeozeroResult<()> {
        processor.polygon_begin(tagged, polygon.rings().len(), polygon_idx)?;
        for (ring_idx, ring) in polygon.rings().iter().enumerate() {
            process_line_string(processor, false, ring_idx, ring)?;
        }
        processor.polygon_end(tagged, polygon_idx)?;
        Ok(())
    }
}

/// Async processing of HTTP selected feature - e.g. to read geomedea features via HTTP and write them to a different
/// format.
///
/// ```no_run
/// # async fn example() {
/// let mut geojson_writer = geozero::geojson::GeoJsonWriter::new(vec![]);
/// let mut http_reader = geomedea::HttpReader::open("https://my-example.example/my-file.geomedea").await.unwrap();
/// let mut feature_stream = http_reader.select_all().await.unwrap();
/// geomedea_geozero::process_geomedea(&mut feature_stream, &mut geojson_writer).await.unwrap()
/// # }
/// ```
pub async fn process_features<W: FeatureProcessor>(
    stream: &mut FeatureStream,
    out: &mut W,
) -> GeozeroResult<()> {
    out.dataset_begin(None)?;
    let mut cnt = 0;
    while let Some(feature) = stream
        .next()
        .await
        .transpose()
        .map_err(|e| geozero::error::GeozeroError::Feature(e.to_string()))?
    {
        processing::process_feature(out, cnt, feature)?;
        cnt += 1;
    }
    out.dataset_end()
}
