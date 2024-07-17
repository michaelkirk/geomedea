mod geozero_reader;
#[cfg(feature = "writer")]
mod geozero_writer;

pub use geozero_reader::process_features as process_geomedea;
pub use geozero_reader::GeozeroReader as GeomedeaReader;

#[cfg(feature = "writer")]
pub use geozero_writer::GeozeroWriter as GeomedeaWriter;

pub use geomedea;
pub use geozero;

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

#[cfg(feature = "writer")]
#[cfg(test)]
mod tests {
    use super::*;
    use geozero::{GeozeroDatasource, ProcessToJson};
    use serde_json::json;

    #[test]
    fn uncompressed_roundtrip_json() {
        roundtrip_json(false)
    }

    #[test]
    fn compressed_roundtrip_json() {
        roundtrip_json(true)
    }

    fn roundtrip_json(is_compressed: bool) {
        let input = json!({
          "type": "FeatureCollection",
          "features": [
            {
              "type": "Feature",
              "geometry": {
                "type": "Point",
                "coordinates": [ -118.2562, 34.1060 ]
              }
            }
          ]
        })
        .to_string();

        let mut geojson = geozero::geojson::GeoJson(&input);
        let mut output = vec![];
        {
            let mut writer = GeomedeaWriter::new(&mut output, is_compressed).unwrap();
            geojson.process(&mut writer).unwrap();
            writer.finish().unwrap();
        }

        let reader = GeomedeaReader::new(&*output).unwrap();
        let mut feature_iter = reader.select_all().unwrap();
        let round_trip = feature_iter.to_json().unwrap();

        let round_trip: serde_json::Value = serde_json::from_str(&round_trip).unwrap();
        let input: serde_json::Value = serde_json::from_str(&input).unwrap();
        assert_eq!(input, round_trip);
    }
}
