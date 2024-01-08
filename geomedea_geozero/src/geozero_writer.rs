use geomedea::{
    Feature, Geometry, GeometryCollection, LineString, LngLat, MultiLineString, MultiPoint,
    MultiPolygon, Polygon, Writer,
};
use geozero::error::{GeozeroError, Result as GeozeroResult};
use geozero::{FeatureProcessor, GeomProcessor, PropertyProcessor};
use std::io::Write;

/// Write geomedea from a geozero data source - e.g. converting a different format to geomedea.
#[derive(Debug)]
pub struct GeozeroWriter<W: Write> {
    inner: Writer<W>,
    current_feature: Option<FeatureBuilder>,
    is_feature_processor: bool,
}

#[derive(Debug)]
struct FeatureBuilder {
    geometry: Option<Geometry>,
    properties: geomedea::Properties,
    geometry_stack: Vec<WIPGeometry>,
}

impl FeatureBuilder {
    fn new() -> Self {
        Self {
            geometry_stack: vec![],
            geometry: None,
            properties: geomedea::Properties::empty(),
        }
    }

    fn set_geometry(&mut self, geometry: Geometry) -> GeozeroResult<()> {
        if let Some(existing) = &self.geometry {
            return Err(GeozeroError::FeatureGeometry(
                format!("Trying to set geometry, but this feature has already set its geometry.\nexisting: {existing:?},\nnew: {geometry:?}")
            ));
        }
        self.geometry = Some(geometry);
        Ok(())
    }

    fn set_property(&mut self, name: &str, value: &geozero::ColumnValue) -> GeozeroResult<()> {
        self.properties
            .insert(name.into(), geozero_to_geomedea_property_value(value));
        Ok(())
    }

    fn finish(self) -> GeozeroResult<Feature> {
        let Some(geometry) = self.geometry else {
            // TODO: do we want to support features w/o geometry?
            log::warn!("feature had no geometry");
            // return Err(GeozeroError::FeatureGeometry(
            //     "feature had no geometry".to_string(),
            // ));
            return Ok(Feature::new(
                geomedea::Geometry::Point(LngLat::degrees(0.0, 0.0)),
                self.properties,
            ));
        };

        Ok(Feature::new(geometry, self.properties))
    }
}

pub fn geomedea_to_geozero_column_value(
    property_value: &geomedea::PropertyValue,
) -> geozero::ColumnValue {
    match property_value {
        geomedea::PropertyValue::Bool(value) => geozero::ColumnValue::Bool(*value),
        geomedea::PropertyValue::Int8(value) => geozero::ColumnValue::Byte(*value),
        geomedea::PropertyValue::UInt8(value) => geozero::ColumnValue::UByte(*value),
        geomedea::PropertyValue::Int16(value) => geozero::ColumnValue::Short(*value),
        geomedea::PropertyValue::UInt16(value) => geozero::ColumnValue::UShort(*value),
        geomedea::PropertyValue::Int32(value) => geozero::ColumnValue::Int(*value),
        geomedea::PropertyValue::UInt32(value) => geozero::ColumnValue::UInt(*value),
        geomedea::PropertyValue::Int64(value) => geozero::ColumnValue::Long(*value),
        geomedea::PropertyValue::UInt64(value) => geozero::ColumnValue::ULong(*value),
        geomedea::PropertyValue::Float32(value) => geozero::ColumnValue::Float(*value),
        geomedea::PropertyValue::Float64(value) => geozero::ColumnValue::Double(*value),
        geomedea::PropertyValue::Bytes(value) => geozero::ColumnValue::Binary(value),
        geomedea::PropertyValue::String(value) => geozero::ColumnValue::String(value),
        geomedea::PropertyValue::Vec(_value) => todo!("handle unsupported"),
        geomedea::PropertyValue::Map(_value) => todo!("handle unsupported"),
    }
}

pub fn geozero_to_geomedea_property_value(
    column_value: &geozero::ColumnValue,
) -> geomedea::PropertyValue {
    match column_value {
        geozero::ColumnValue::Byte(value) => geomedea::PropertyValue::Int8(*value),
        geozero::ColumnValue::UByte(value) => geomedea::PropertyValue::UInt8(*value),
        geozero::ColumnValue::Bool(value) => geomedea::PropertyValue::Bool(*value),
        geozero::ColumnValue::Short(value) => geomedea::PropertyValue::Int16(*value),
        geozero::ColumnValue::UShort(value) => geomedea::PropertyValue::UInt16(*value),
        geozero::ColumnValue::Int(value) => geomedea::PropertyValue::Int32(*value),
        geozero::ColumnValue::UInt(value) => geomedea::PropertyValue::UInt32(*value),
        geozero::ColumnValue::Long(value) => geomedea::PropertyValue::Int64(*value),
        geozero::ColumnValue::ULong(value) => geomedea::PropertyValue::UInt64(*value),
        geozero::ColumnValue::Float(value) => geomedea::PropertyValue::Float32(*value),
        geozero::ColumnValue::Double(value) => geomedea::PropertyValue::Float64(*value),
        geozero::ColumnValue::String(value) => geomedea::PropertyValue::String(value.to_string()),
        geozero::ColumnValue::Json(value) => geomedea::PropertyValue::String(value.to_string()),
        geozero::ColumnValue::DateTime(value) => geomedea::PropertyValue::String(value.to_string()),
        geozero::ColumnValue::Binary(value) => geomedea::PropertyValue::Bytes(value.to_vec()),
    }
}

impl<W: Write> GeozeroWriter<W> {
    pub fn new(writer: W, is_compressed: bool) -> geozero::error::Result<Self> {
        let writer = Writer::new(writer, is_compressed)
            .map_err(|e| geozero::error::GeozeroError::Dataset(e.to_string()))?;
        // In the case that Self is being used as a GeomProcessor, we create
        // a single FeatureBuilder by default.
        //
        // However, in the case we're being used as a
        // FeatureProcessor, we want a FeatureBuilder per Feature, so we'll immediately clear
        // this default `current_feature` upon starting the dataset, so that it can
        // be set explicitly by the FeatureProcessor for each Feature.
        let mut feature_builder = FeatureBuilder::new();
        feature_builder
            .geometry_stack
            .push(WIPGeometry::geometrycollection_begin(0));

        Ok(Self {
            inner: writer,
            current_feature: Some(feature_builder),
            is_feature_processor: false,
        })
    }

    pub fn set_page_size_goal(&mut self, bytes: u64) {
        self.inner.set_page_size_goal(bytes);
    }

    pub fn finish(mut self) -> GeozeroResult<()> {
        if self.is_feature_processor {
            assert!(
                self.current_feature.is_none(),
                "unfinished Feature in FeatureProcessor: {:?}",
                self.current_feature
            );
        } else {
            let Some(mut geometry_processor_builder) = self.current_feature else {
                todo!("handle missing geometry processor collection");
            };
            geometry_processor_builder.geometrycollection_end(0)?;
            let mut feature = geometry_processor_builder.finish()?;
            if let Geometry::GeometryCollection(geometry_collection) = feature.geometry_mut() {
                if geometry_collection.geometries().len() == 1 {
                    let mut child = geometry_collection.pop().unwrap();
                    std::mem::swap(feature.geometry_mut(), &mut child);
                }

                self.inner
                    .add_feature(&feature)
                    .map_err(|e| GeozeroError::Feature(e.to_string()))?;
            } else {
                todo!("handle unexpected geometry singleton for geomprocessor");
            }
        }
        self.inner
            .finish()
            .map_err(|e| GeozeroError::Dataset(e.to_string()))?;

        Ok(())
    }
}

impl<W: Write> PropertyProcessor for GeozeroWriter<W> {
    fn property(
        &mut self,
        _idx: usize,
        name: &str,
        value: &geozero::ColumnValue,
    ) -> GeozeroResult<bool> {
        let Some(current_feature) = &mut self.current_feature else {
            return Err(no_feature_started("property"));
        };
        current_feature.set_property(name, value)?;
        Ok(false)
    }
}

impl<W: Write> FeatureProcessor for GeozeroWriter<W> {
    fn dataset_begin(&mut self, _name: Option<&str>) -> GeozeroResult<()> {
        // remove default configuration used only for non-feature GeometryProcessor
        self.is_feature_processor = true;
        self.current_feature = None;
        Ok(())
    }

    fn feature_begin(&mut self, _idx: u64) -> GeozeroResult<()> {
        self.current_feature = Some(FeatureBuilder::new());
        Ok(())
    }

    fn feature_end(&mut self, _idx: u64) -> GeozeroResult<()> {
        let Some(feature_builder) = self.current_feature.take() else {
            return Err(GeozeroError::Feature(
                "ended a Feature without first starting it".to_string(),
            ));
        };
        let feature = feature_builder.finish()?;
        self.inner
            .add_feature(&feature)
            .map_err(|e| GeozeroError::Feature(e.to_string()))
    }
}

fn no_feature_started(method_name: &str) -> GeozeroError {
    GeozeroError::Feature(format!(
        "called {method_name} though no feature was in progress"
    ))
}

macro_rules! delegate_to_current_feature {
    ($(fn $fn:ident(&mut self $(, $arg:ident : $ty:ty)*);)+) => {
        $(fn $fn(&mut self$(, $arg: $ty)*) -> GeozeroResult<()> {
            let Some(current_feature) = &mut self.current_feature else {
                return Err(no_feature_started(&format!("`{}`", stringify!($fn))));
            };
            current_feature.$fn($($arg,)*)
        })+
    }

}
impl<W: Write> GeomProcessor for GeozeroWriter<W> {
    delegate_to_current_feature! {
        fn xy(&mut self, x: f64, y: f64, idx: usize);
        fn point_begin(&mut self, idx: usize);
        fn point_end(&mut self, idx: usize);
        fn linestring_begin(&mut self, tagged: bool, size: usize, idx: usize);
        fn linestring_end(&mut self, tagged: bool, idx: usize);
        fn polygon_begin(&mut self, tagged: bool, size: usize, idx: usize);
        fn polygon_end(&mut self, tagged: bool, idx: usize);
        fn multipoint_begin(&mut self, size: usize, idx: usize);
        fn multipoint_end(&mut self, idx: usize);
        fn multilinestring_begin(&mut self, size: usize, idx: usize);
        fn multilinestring_end(&mut self, idx: usize);
        fn multipolygon_begin(&mut self, size: usize, idx: usize);
        fn multipolygon_end(&mut self, idx: usize);
        fn geometrycollection_begin(&mut self, size: usize, _idx: usize);
        fn geometrycollection_end(&mut self, idx: usize);
    }
}

impl GeomProcessor for FeatureBuilder {
    fn xy(&mut self, x: f64, y: f64, idx: usize) -> geozero::error::Result<()> {
        let Some(wip_geometry) = self.geometry_stack.last_mut() else {
            return Err(GeozeroError::Geometry(
                "called xy though no geometry was in progress".to_string(),
            ));
        };
        wip_geometry.xy(x, y, idx)
    }

    fn point_begin(&mut self, _idx: usize) -> geozero::error::Result<()> {
        log::trace!("point_begin");
        // TODO: verify stack makes sense, e.g. empty or GeometryCollection
        self.geometry_stack.push(WIPGeometry::point_begin());
        Ok(())
    }

    fn point_end(&mut self, _idx: usize) -> GeozeroResult<()> {
        log::trace!("point_end");
        let Some(wip_geometry) = self.geometry_stack.pop() else {
            return Err(GeozeroError::Geometry(
                "called point_end though no geometry was in progress".to_string(),
            ));
        };

        let point = wip_geometry.point_end()?;
        match self.geometry_stack.last_mut() {
            None => self.set_geometry(Geometry::Point(point)),
            Some(wip_geometry) => match wip_geometry {
                WIPGeometry::GeometryCollection(geometry_collection) => {
                    geometry_collection.push(Geometry::Point(point));
                    Ok(())
                }
                other => Err(GeozeroError::Geometry(format!(
                    "Points can only be nested within a GeometryCollection. Found: {other:?}"
                ))),
            },
        }
    }

    fn linestring_begin(
        &mut self,
        _tagged: bool,
        size: usize,
        _idx: usize,
    ) -> geozero::error::Result<()> {
        log::trace!("linestring_begin");
        // TODO: verify stack makes sense for line string
        // empty, polygon, multi_linestring, geometry_collection
        self.geometry_stack
            .push(WIPGeometry::linestring_begin(size));
        Ok(())
    }

    fn linestring_end(&mut self, _tagged: bool, idx: usize) -> GeozeroResult<()> {
        log::trace!("linestring_end");
        let Some(wip_geometry) = self.geometry_stack.pop() else {
            return Err(GeozeroError::Geometry(
                "called linestring_end though no geometry was in progress".to_string(),
            ));
        };

        let line_string = wip_geometry.linestring_end()?;
        match self.geometry_stack.last_mut() {
            None => {
                self.set_geometry(Geometry::LineString(line_string))?;
            }
            Some(wip_geometry) => match wip_geometry {
                WIPGeometry::Polygon(polygon) => polygon.push_ring(line_string),
                WIPGeometry::MultiLineString(multi_line_string) => {
                    log::debug!("Finished LineString #{idx} for MultiLineString");
                    multi_line_string.push(line_string);
                }
                WIPGeometry::GeometryCollection(geometry_collection) => {
                    log::debug!("Finished LineString #{idx} for GeometryCollection");
                    geometry_collection.push(Geometry::LineString(line_string));
                }
                other => {
                    return Err(GeozeroError::Geometry(format!(
                        "Unexpectedly ended a LineString while in the middle of a {other:?}"
                    )));
                }
            },
        }
        Ok(())
    }

    fn polygon_begin(
        &mut self,
        _tagged: bool,
        size: usize,
        _idx: usize,
    ) -> geozero::error::Result<()> {
        log::trace!("polygon_begin");
        // TODO: verify stack makes sense for polygon
        // empty, multi_polygon, geometry_collection
        self.geometry_stack.push(WIPGeometry::polygon_begin(size));
        Ok(())
    }

    fn polygon_end(&mut self, tagged: bool, idx: usize) -> geozero::error::Result<()> {
        log::trace!("polygon_end");
        let Some(wip_geometry) = self.geometry_stack.pop() else {
            return Err(GeozeroError::Geometry(
                "called polygon_end though no geometry was in progress".to_string(),
            ));
        };
        let polygon = wip_geometry.polygon_end()?;
        match self.geometry_stack.last_mut() {
            None => {
                debug_assert!(tagged);
                self.set_geometry(Geometry::Polygon(polygon))?;
            }
            Some(parent) => match parent {
                WIPGeometry::MultiPolygon(multi_polygon) => {
                    debug_assert!(!tagged);
                    log::debug!("Finished Polygon #{idx} for MultiPolygon");
                    multi_polygon.push(polygon);
                }
                WIPGeometry::GeometryCollection(geometry_collection) => {
                    log::debug!("Finished Polygon #{idx} for GeometryCollection");
                    geometry_collection.push(Geometry::Polygon(polygon));
                }
                other => {
                    return Err(GeozeroError::Geometry(format!(
                        "Unexpectedly ended a Polygon while in the middle of a {other:?}"
                    )));
                }
            },
        }
        Ok(())
    }

    fn multipoint_begin(&mut self, size: usize, _idx: usize) -> geozero::error::Result<()> {
        log::trace!("multipoint_begin");
        // TODO: verify stack makes sense for multipoint
        // empty or geometry_collection
        self.geometry_stack
            .push(WIPGeometry::multipoint_begin(size));
        Ok(())
    }

    fn multipoint_end(&mut self, idx: usize) -> geozero::error::Result<()> {
        log::trace!("multipoint_end");
        let Some(wip_geometry) = self.geometry_stack.pop() else {
            return Err(GeozeroError::Geometry(
                "called multipoint_end though no geometry was in progress".to_string(),
            ));
        };
        let multi_point = wip_geometry.multipoint_end()?;
        match self.geometry_stack.last_mut() {
            None => {
                self.set_geometry(Geometry::MultiPoint(multi_point))?;
            }
            Some(parent) => match parent {
                WIPGeometry::GeometryCollection(geometry_collection) => {
                    log::debug!("Finished MultiPoint #{idx} for GeometryCollection");
                    geometry_collection.push(Geometry::MultiPoint(multi_point))
                }
                other => todo!("handle other {other:?}"),
            },
        }

        Ok(())
    }

    fn multilinestring_begin(&mut self, size: usize, _idx: usize) -> geozero::error::Result<()> {
        log::trace!("multipoint_begin");
        // TODO: verify stack makes sense for MultiLineString: empty or geometry_collection
        self.geometry_stack
            .push(WIPGeometry::multilinestring_begin(size));
        Ok(())
    }

    fn multilinestring_end(&mut self, idx: usize) -> geozero::error::Result<()> {
        log::trace!("multilinestring_end");
        let Some(wip_geometry) = self.geometry_stack.pop() else {
            return Err(GeozeroError::Geometry(
                "called multilinestring_end though no geometry was in progress".to_string(),
            ));
        };
        let multi_line_string = wip_geometry.multilinestring_end()?;
        match self.geometry_stack.last_mut() {
            None => {
                self.set_geometry(Geometry::MultiLineString(multi_line_string))?;
            }
            Some(parent) => match parent {
                WIPGeometry::GeometryCollection(geometry_collection) => {
                    log::debug!("Finished MultiLineString #{idx} for GeometryCollection");
                    geometry_collection.push(Geometry::MultiLineString(multi_line_string))
                }
                other => todo!("handle unexpected geometry: {other:?}"),
            },
        }
        Ok(())
    }

    fn multipolygon_begin(&mut self, size: usize, _idx: usize) -> geozero::error::Result<()> {
        log::trace!("multipolygon_begin");
        // TODO: verify stack makes sense for MultiPolygon: empty or geometry_collection
        self.geometry_stack
            .push(WIPGeometry::multipolygon_begin(size));
        Ok(())
    }

    fn multipolygon_end(&mut self, idx: usize) -> geozero::error::Result<()> {
        log::trace!("multipolygon_end");
        let Some(wip_geometry) = self.geometry_stack.pop() else {
            return Err(GeozeroError::Geometry(
                "called multipolygon_end though no geometry was in progress".to_string(),
            ));
        };
        let multi_polygon = wip_geometry.multipolygon_end()?;
        match self.geometry_stack.last_mut() {
            None => {
                self.set_geometry(Geometry::MultiPolygon(multi_polygon))?;
            }
            Some(parent) => match parent {
                WIPGeometry::GeometryCollection(geometry_collection) => {
                    log::debug!("Finished MultiPolygon #{idx} for GeometryCollection");
                    geometry_collection.push(Geometry::MultiPolygon(multi_polygon))
                }
                other => todo!("handle unexpected geometry: {other:?}"),
            },
        }
        Ok(())
    }

    fn geometrycollection_begin(&mut self, size: usize, _idx: usize) -> geozero::error::Result<()> {
        log::trace!("geometrycollection_begin");
        // TODO: verify stack makes sense for GeometryColletion: empty or geometry_collection
        self.geometry_stack
            .push(WIPGeometry::geometrycollection_begin(size));
        Ok(())
    }

    fn geometrycollection_end(&mut self, idx: usize) -> geozero::error::Result<()> {
        log::trace!("geometrycollection_end");
        let Some(wip_geometry) = self.geometry_stack.pop() else {
            return Err(GeozeroError::Geometry(
                "called geometrycollection_end though no geometry was in progress".to_string(),
            ));
        };
        let geometry_collection = wip_geometry.geometrycollection_end()?;
        match self.geometry_stack.last_mut() {
            None => {
                self.set_geometry(Geometry::GeometryCollection(geometry_collection))?;
            }
            Some(parent) => match parent {
                WIPGeometry::GeometryCollection(parent_geometry_collection) => {
                    log::debug!("Finished GeometryCollection #{idx} for GeometryCollection");
                    parent_geometry_collection
                        .push(Geometry::GeometryCollection(geometry_collection))
                }
                other => todo!("handle unexpected geometry: {other:?}"),
            },
        }
        Ok(())
    }
}

#[derive(Debug)]
enum WIPGeometry {
    Point(Option<LngLat>),
    LineString(LineString),
    Polygon(Polygon),
    MultiPoint(MultiPoint),
    MultiLineString(MultiLineString),
    MultiPolygon(MultiPolygon),
    GeometryCollection(GeometryCollection),
}

impl WIPGeometry {
    fn xy(&mut self, x: f64, y: f64, _idx: usize) -> GeozeroResult<()> {
        let lng_lat = LngLat::degrees(x, y);
        match self {
            WIPGeometry::Point(Some(_lng_lat)) => {
                return Err(GeozeroError::Geometry(
                    "xy called multiple times for Point geometry".to_string(),
                ));
            }
            WIPGeometry::Point(None) => {
                *self = WIPGeometry::Point(Some(lng_lat));
            }
            WIPGeometry::LineString(line_string) => {
                line_string.push_point(lng_lat);
            }
            WIPGeometry::Polygon(_polygon) => {
                return Err(GeozeroError::Geometry(
                    "xy called for polygon without first starting a ring (LineString)".to_string(),
                ));
            }
            WIPGeometry::MultiPoint(multi_point) => multi_point.push(lng_lat),
            WIPGeometry::MultiLineString(_multi_line_string) => {
                return Err(GeozeroError::Geometry(
                    "xy called for MultiLineString without first starting a LineString".to_string(),
                ));
            }
            WIPGeometry::MultiPolygon(_multi_polygon) => {
                return Err(GeozeroError::Geometry(
                    "xy called for MultiPolygon without first starting a Polygon".to_string(),
                ));
            }
            WIPGeometry::GeometryCollection(_geometry_collection) => {
                return Err(GeozeroError::Geometry("xy called for GeometryCollection without first starting a child Geometry that accepts points".to_string()));
            }
        }
        Ok(())
    }

    fn point_begin() -> Self {
        WIPGeometry::Point(None)
    }

    fn point_end(self) -> GeozeroResult<LngLat> {
        match self {
            WIPGeometry::Point(Some(lng_lat)) => Ok(lng_lat),
            WIPGeometry::Point(None) => Err(GeozeroError::Geometry(
                "xy never set for Point geometry".to_string(),
            )),
            other => Err(GeozeroError::Geometry(format!(
                "end_point for non Point geometry: {other:?}"
            ))),
        }
    }

    fn linestring_begin(size: usize) -> Self {
        WIPGeometry::LineString(LineString::new(Vec::with_capacity(size)))
    }

    fn linestring_end(self) -> geozero::error::Result<LineString> {
        match self {
            WIPGeometry::LineString(line_string) => Ok(line_string),
            other => Err(GeozeroError::Geometry(format!(
                "linestring_end for non LineString geometry: {other:?}"
            ))),
        }
    }

    fn polygon_begin(size: usize) -> Self {
        WIPGeometry::Polygon(Polygon::new(Vec::with_capacity(size)))
    }

    fn polygon_end(self) -> GeozeroResult<Polygon> {
        match self {
            WIPGeometry::Polygon(polygon) => Ok(polygon),
            other => Err(GeozeroError::Geometry(format!(
                "polygon_end for non Polygon geometry: {other:?}"
            ))),
        }
    }

    fn multipoint_begin(size: usize) -> Self {
        WIPGeometry::MultiPoint(MultiPoint::new(Vec::with_capacity(size)))
    }

    fn multipoint_end(self) -> GeozeroResult<MultiPoint> {
        match self {
            WIPGeometry::MultiPoint(multi_point) => Ok(multi_point),
            other => Err(GeozeroError::Geometry(format!(
                "multipoint_end for non MultiPoint geometry: {other:?}"
            ))),
        }
    }

    fn multilinestring_begin(size: usize) -> Self {
        WIPGeometry::MultiLineString(MultiLineString::new(Vec::with_capacity(size)))
    }

    fn multilinestring_end(self) -> GeozeroResult<MultiLineString> {
        match self {
            WIPGeometry::MultiLineString(multi_line_string) => Ok(multi_line_string),
            other => Err(GeozeroError::Geometry(format!(
                "multilinestring_end for non MultiLineString geometry: {other:?}"
            ))),
        }
    }

    fn multipolygon_begin(size: usize) -> Self {
        WIPGeometry::MultiPolygon(MultiPolygon::new(Vec::with_capacity(size)))
    }

    fn multipolygon_end(self) -> GeozeroResult<MultiPolygon> {
        match self {
            WIPGeometry::MultiPolygon(multi_polygon) => Ok(multi_polygon),
            other => Err(GeozeroError::Geometry(format!(
                "multipolygon_end for non MultiPolygon geometry: {other:?}"
            ))),
        }
    }

    fn geometrycollection_begin(size: usize) -> Self {
        WIPGeometry::GeometryCollection(GeometryCollection::new(Vec::with_capacity(size)))
    }

    fn geometrycollection_end(self) -> GeozeroResult<GeometryCollection> {
        match self {
            WIPGeometry::GeometryCollection(geometry_collection) => Ok(geometry_collection),
            other => Err(GeozeroError::Geometry(format!(
                "geometrycollection_end for non GeometryCollection geometry: {other:?}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geozero_reader::GeozeroReader;
    use approx::assert_relative_eq;
    use geomedea::Reader;
    use geozero::{GeozeroDatasource, GeozeroGeometry};
    use std::fs::File;
    use std::io::BufReader;

    #[test]
    fn from_larger_json_uncompressed() {
        from_larger_json(false)
    }

    #[test]
    fn from_larger_json_compressed() {
        from_larger_json(true)
    }

    fn from_larger_json(is_compressed: bool) {
        let input = BufReader::new(File::open("../test_fixtures/places.json").unwrap());
        let mut geojson = geozero::geojson::GeoJsonReader(input);

        let mut output = vec![];
        let mut writer = GeozeroWriter::new(&mut output, is_compressed).unwrap();
        geojson.process(&mut writer).unwrap();
        writer.finish().unwrap();

        let reader = Reader::new(output.as_slice()).unwrap();
        let mut features = reader.select_all().unwrap();
        let first = features.try_next().unwrap().unwrap();
        let Geometry::Point(lnglat) = first.geometry() else {
            panic!("unexpected geometry");
        };
        assert_relative_eq!(lnglat.lng_degrees(), 176.99445209423166, epsilon = 1e-7);
        assert_relative_eq!(lnglat.lat_degrees(), -89.99999981438727, epsilon = 1e-7);

        if is_compressed {
            assert_eq!(output.len(), 63291);
        } else {
            assert_eq!(output.len(), 193771);
        }
    }

    #[test]
    fn select_all_from_larger_json_uncompressed() {
        select_all_from_larger_json(false)
    }

    #[test]
    fn select_all_from_larger_json_compressed() {
        select_all_from_larger_json(true)
    }

    fn select_all_from_larger_json(is_compressed: bool) {
        let input = BufReader::new(File::open("../test_fixtures/places.json").unwrap());
        let mut geojson = geozero::geojson::GeoJsonReader(input);

        let mut output = vec![];
        let mut writer = GeozeroWriter::new(&mut output, is_compressed).unwrap();
        writer.set_page_size_goal(8 * 1024);
        geojson.process(&mut writer).unwrap();
        writer.finish().unwrap();

        let reader = Reader::new(output.as_slice()).unwrap();
        let mut features = reader.select_all().unwrap();
        let first = features.try_next().unwrap().unwrap();
        let Geometry::Point(lnglat) = first.geometry() else {
            panic!("unexpected geometry");
        };
        assert_relative_eq!(lnglat.lng_degrees(), 176.99445209423166, epsilon = 1e-7);
        assert_relative_eq!(lnglat.lat_degrees(), -89.99999981438727, epsilon = 1e-7);

        if is_compressed {
            assert_eq!(output.len(), 66525);
        } else {
            assert_eq!(output.len(), 193963);
        }
    }

    #[test]
    fn polygons_uncompressed() {
        test_polygons(false)
    }
    #[test]
    fn polygons_compressed() {
        test_polygons(true)
    }

    fn test_polygons(is_compressed: bool) {
        let input = BufReader::new(File::open("../test_fixtures/countries.geojson").unwrap());
        let mut geojson = geozero::geojson::GeoJsonReader(input);

        let mut output = vec![];
        let mut writer = GeozeroWriter::new(&mut output, is_compressed).unwrap();
        writer.set_page_size_goal(16 * 1024);
        geojson.process(&mut writer).unwrap();
        writer.finish().unwrap();

        let reader = Reader::new(output.as_slice()).unwrap();
        let bounds = geomedea::Bounds::from_corners(
            &LngLat::degrees(24.0, -4.0),
            &LngLat::degrees(24.5, -3.5),
        );
        let mut features = reader.select_bbox(&bounds).unwrap();
        let first = features.try_next().unwrap().unwrap();
        let Geometry::Polygon(_polygon) = first.geometry() else {
            panic!("unexpected polygon geometry: {first:?}");
        };
        let name = first.property("name").unwrap();
        assert_eq!(
            &geomedea::PropertyValue::from("Democratic Republic of the Congo"),
            name
        );
        if is_compressed {
            assert_eq!(output.len(), 81600);
        } else {
            assert_eq!(output.len(), 108715);
        }
    }

    #[test]
    fn convert_all_test_fixtures() {
        for entry in std::fs::read_dir("../test_fixtures/canonical-geojson").unwrap() {
            let entry_path = entry.unwrap().path();
            let path = entry_path.to_str().unwrap();
            dbg!(&path);
            if path.contains("3d") {
                log::info!("skipping 3d test {path:?} for now");
                continue;
            }

            if path.contains("nullgeometry") {
                log::info!("skipping nullgeometry test {path:?} for now");
                continue;
            }

            let input = BufReader::new(File::open(path).unwrap());
            let mut geojson = geozero::geojson::GeoJsonReader(input);
            let mut output = vec![];
            {
                let mut writer = GeozeroWriter::new(&mut output, false).unwrap();
                geojson.process(&mut writer).unwrap();
            }
        }
    }

    #[test]
    fn geometry_collection_with_all_geometries() {
        let collection = serde_json::json!({
            "type": "FeatureCollection",
            "features": [
                {
                    "type": "Feature",
                    "geometry": {
                        "type": "GeometryCollection",
                        "geometries": [
                            {
                                "type": "Point",
                                "coordinates": [1.0, 2.0]
                            },
                            {
                                "type": "LineString",
                                "coordinates": [[1.0, 2.0], [3.0, 4.0]]
                            },
                            {
                                "type": "Polygon",
                                "coordinates": [
                                    [ [100.0, 0.0], [101.0, 0.0], [101.0, 1.0], [100.0, 1.0], [100.0, 0.0] ]
                                ]
                            },
                            {
                                "type": "MultiPoint",
                                "coordinates": [[100.0, 1.0], [200.0, 2.0]]
                            },
                            {
                                "type": "MultiLineString",
                                "coordinates": [
                                    [ [100.0, 0.0], [101.0, 1.0] ],
                                    [ [102.0, 2.0], [103.0, 3.0] ]
                                ]
                            },
                            {
                                "type": "MultiPolygon",
                                "coordinates": [
                                    [
                                        [[102.0, 2.0], [103.0, 2.0], [103.0, 3.0], [102.0, 3.0], [102.0, 2.0]]
                                    ],
                                    [
                                        [[100.0, 0.0], [101.0, 0.0], [101.0, 1.0], [100.0, 1.0], [100.0, 0.0]],
                                        [[100.2, 0.2], [100.2, 0.8], [100.8, 0.8], [100.8, 0.2], [100.2, 0.2]]
                                    ]
                                ]
                            },
                            {
                                "type": "GeometryCollection",
                                "geometries": [
                                    {
                                        "type": "Point",
                                        "coordinates": [100.0, 0.0]
                                    },
                                    {
                                        "type": "LineString",
                                        "coordinates": [ [101.0, 0.0], [102.0, 1.0] ]
                                    }
                                ]
                            }
                        ]
                    }
                }

            ]
        })
            .to_string();

        let input = geozero::geojson::GeoJson(&collection);

        let mut output = vec![];
        let mut geomedea_writer = GeozeroWriter::new(&mut output, false).unwrap();
        input.process_geom(&mut geomedea_writer).unwrap();
        // should this finish be in process_geom?
        geomedea_writer.finish().unwrap();
        assert_eq!(output.len(), 505);
    }

    #[test]
    fn roundtrip_all_test_fixtures() {
        let mut tests_ran = 0;
        for entry in std::fs::read_dir("../test_fixtures/canonical-geojson").unwrap() {
            dbg!(&entry);

            let path = entry.unwrap().path();
            let path = path.to_str().unwrap();
            if path.contains("3d") {
                log::info!("skipping 3d test {path:?} for now");
                continue;
            }

            if path.contains("nullgeometry") {
                log::info!("skipping nullgeometry test {path:?} for now");
                continue;
            }

            test_round_trip(path);
            tests_ran += 1;
        }
        assert_eq!(tests_ran, 13);
    }

    #[test]
    fn focused_round_trip() {
        // test_round_trip("../test_fixtures/canonical-geojson/good-point.geojson");
        test_round_trip("../test_fixtures/canonical-geojson/good-featurecollection-bbox.geojson")
    }

    fn test_round_trip(path: &str) {
        let expected: serde_json::Value = {
            let input = BufReader::new(File::open(path).unwrap());
            // let mut geojson_reader = geozero_integration::geojson::GeoJsonReader(input);
            // let mut roundtrip = vec![];
            // let mut geojson_writer = GeoJsonWriter::new(&mut roundtrip);
            // geojson_reader.process(&mut geojson_writer).unwrap();
            // assert_eq!(roundtrip.len(), 271);
            //
            // let geojson_string = String::from_utf8(roundtrip.clone()).unwrap();
            // dbg!(&geojson_string);
            // println!("footch: {geojson_string}");
            //
            // // THIS IS BROKEN: because geozero_integration can't roundtrip geojson without inserting
            // // a spurious comma between the `geometry` key like: `"geometry": ,{ "type": "Point", ...}`
            // serde_json::from_reader(geojson_string.as_bytes()).unwrap()
            serde_json::from_reader(input).unwrap()
        };

        let input = BufReader::new(File::open(path).unwrap());
        let mut geojson = geozero::geojson::GeoJsonReader(input);
        let mut geomedia_output = vec![];
        {
            let mut writer = GeozeroWriter::new(&mut geomedia_output, false).unwrap();
            geojson.process(&mut writer).unwrap();
            writer.finish().unwrap()
        }

        let geomedea_reader = GeozeroReader::new(geomedia_output.as_slice()).unwrap();
        let mut feature_iter = geomedea_reader.select_all().unwrap();

        use geozero::geojson::GeoJsonWriter;
        let mut roundtripped_output = vec![];
        let mut geojson_writer = GeoJsonWriter::new(&mut roundtripped_output);
        feature_iter.process(&mut geojson_writer).unwrap();

        println!(
            "{}",
            String::from_utf8(roundtripped_output.clone()).unwrap()
        );

        let actual: serde_json::Value =
            serde_json::from_reader(roundtripped_output.as_slice()).unwrap();

        assert_eq_geojson(actual, expected);
    }

    fn assert_eq_geojson(actual: serde_json::Value, expected: serde_json::Value) {
        // dbg!(&actual);
        let normalized_actual = normalized::FeatureCollection::from(actual);
        let normalized_expected = normalized::FeatureCollection::from(expected);

        assert_eq!(normalized_actual, normalized_expected);
    }

    pub(crate) mod normalized {
        type Feature = serde_json::Value;

        #[derive(PartialEq, Debug)]
        pub struct FeatureCollection {
            features: Vec<Feature>,
        }

        impl From<serde_json::Value> for FeatureCollection {
            fn from(geojson: serde_json::Value) -> Self {
                let serde_json::Value::Object(mut geojson_object) = geojson else {
                    panic!("expected map, got {geojson:?}");
                };
                let geojson_type = geojson_object.get("type").unwrap();
                let serde_json::Value::String(geojson_type) = geojson_type else {
                    panic!("unexpected value for `type`: {geojson_type}");
                };

                match geojson_type.as_str() {
                    "FeatureCollection" => {
                        let serde_json::Value::Array(mut features) =
                            geojson_object.remove("features").unwrap()
                        else {
                            panic!("unexpected value for `features`: {geojson_object:?}");
                        };
                        for feature in &mut features {
                            normalize_feature(feature);
                        }
                        // Geomedea will reorder features by their hilbert ordering
                        // so we need *some* consistent wway of sorting to compare the re-ordered features.
                        features.sort_by_key(|geojson| {
                            let serde_json::Value::Object(object) = geojson else {
                                panic!("unexpected feature: {geojson:?}");
                            };
                            let serde_json::Value::Object(geometry) = object.get("geometry").unwrap() else {
                                panic!("unexpected geometry: {object:?}");
                            };

                            let serde_json::Value::Array(coords) = geometry.get("coordinates").unwrap() else {
                                panic!("unexpected coords: {geometry:?}");
                            };

                            fn sum_flattened_array(array: &Vec<serde_json::Value>) -> f64 {
                                let mut sum = 0f64;
                                for element in array {
                                    match &element {
                                        serde_json::Value::Number(coord) => {
                                            sum += coord.as_f64().unwrap();
                                        }
                                        serde_json::Value::Array(coords) => {
                                            sum += sum_flattened_array(coords)
                                        }
                                        other => panic!("expected only coordinates or Vec of coordinates but got {other:?}")
                                    }
                                }
                                sum
                            }

                            sum_flattened_array(coords) as i64
                        });
                        FeatureCollection { features }
                    }
                    "Feature" => {
                        // Feature to Collection since processing geomedea back to geojson will always output a feature collection.
                        let mut feature_value = serde_json::Value::Object(geojson_object);
                        normalize_feature(&mut feature_value);
                        let features = vec![feature_value];
                        FeatureCollection { features }
                    }
                    "Point" | "LineString" | "Polygon" | "MultiPoint" | "MultiLineString"
                    | "MultiPolygon" | "GeometryCollection" => {
                        // promote Geometry to FeatureCollection with one Feature since processing geomedea back to geojson will always output a feature collection.
                        let mut geometry_value = serde_json::Value::Object(geojson_object);
                        normalize_geometry(&mut geometry_value);
                        let feature = serde_json::json!(
                            {
                                    "type": "Feature",
                                    "geometry": geometry_value
                            }
                        );
                        let features = vec![feature];
                        FeatureCollection { features }
                    }
                    other => todo!("handle {other:?}"),
                }
            }
        }

        fn normalize_number(number: &mut serde_json::Number) {
            // Currently we're panicking with integer vs float representations in serde
            let mut normalized = serde_json::Number::from_f64(number.as_f64().unwrap()).unwrap();
            std::mem::swap(number, &mut normalized);
        }

        fn normalize_coordinate_array(array: &mut [serde_json::Value]) {
            for coordinate in array {
                match coordinate {
                    // multidimensional, e.g. MultiPolygon
                    serde_json::Value::Array(coordinates) => {
                        normalize_coordinate_array(coordinates)
                    }
                    serde_json::Value::Number(number) => {
                        normalize_number(number);
                    }
                    _ => panic!("unexpected coordinate {coordinate:?}"),
                };
            }
        }

        fn normalize_feature(feature: &mut serde_json::Value) {
            let serde_json::Value::Object(ref mut feature_object) = feature else {
                panic!("unexpected feature value: {feature:?}");
            };
            remove_foreign_members(feature_object);

            // TODO: normalize properties
            // e.g. geozero_integration has a JSON type which is going to be difficult to reconcile.
            // Though maybe roundtrip through a Map?

            if let Some(properties) = feature_object.get_mut("properties") {
                normalize_properties(properties);
            }

            let geometry = feature_object.get_mut("geometry").unwrap();
            normalize_geometry(geometry)
        }

        fn normalize_properties(properties: &mut serde_json::Value) {
            let serde_json::Value::Object(ref mut properties_object) = properties else {
                panic!("unexpected properties value: {properties:?}");
            };

            for (_key, value) in properties_object {
                if let serde_json::Value::Object(object_value) = value {
                    // might be nice to map this to a Hash or something...
                    log::debug!("normalizing nested json to string");
                    let json_string = serde_json::to_string(object_value).unwrap();
                    std::mem::swap(value, &mut serde_json::Value::String(json_string));
                }
                if let serde_json::Value::Number(number_value) = value {
                    normalize_number(number_value);
                }
            }
        }
        fn normalize_geometry(geometry: &mut serde_json::Value) {
            let serde_json::Value::Object(geometry_object) = geometry else {
                panic!("unexpected geometry value: {geometry:?}");
            };

            remove_foreign_members(geometry_object);

            match geometry_object
                .get("type")
                .unwrap()
                .as_str()
                .expect("string `type`")
            {
                "GeometryCollection" => {
                    let geometries = geometry_object
                        .get_mut("geometries")
                        .unwrap()
                        .as_array_mut()
                        .expect("geometries children");
                    for child_geometry in geometries {
                        normalize_geometry(child_geometry)
                    }
                }
                _other => {
                    let serde_json::Value::Array(coordinates) =
                        geometry_object.get_mut("coordinates").unwrap()
                    else {
                        panic!("unexpected `coordinates` type for geometry: {geometry_object:?}");
                    };

                    normalize_coordinate_array(coordinates);
                }
            }
        }

        fn remove_foreign_members(geojson_object: &mut serde_json::Map<String, serde_json::Value>) {
            // We don't support `bbox`, foreign members, etc.
            // NOTE: this test helper assumes objects don't have confusing keys, e.g. technically a
            // Feature could legally have a `coordinates` foreign member, rather than on this
            // Geometry, but this would strip it out.
            let allowed_keys = [
                "geometry",
                "features",
                "type",
                "properties",
                "coordinates",
                "geometries",
            ];
            let to_delete: Vec<_> = geojson_object
                .keys()
                .filter_map(|key| {
                    if allowed_keys.contains(&key.as_str()) {
                        None
                    } else {
                        Some(key.to_string())
                    }
                })
                .collect();

            for key in to_delete {
                let removed = geojson_object.remove(&key);
                log::trace!("removed {removed:?}");
            }
        }
    }
}
