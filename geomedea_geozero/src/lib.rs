mod geozero_reader;
mod geozero_writer;

pub use geozero_reader::process_features as process_geomedea;
pub use geozero_reader::GeozeroReader as GeomedeaReader;
pub use geozero_writer::GeozeroWriter as GeomedeaWriter;

pub use geomedea;
pub use geozero;

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
