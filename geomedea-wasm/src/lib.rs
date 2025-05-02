mod to_geo_type;
mod utils;

use crate::to_geo_type::ToGeoType;

use futures_util::stream::StreamExt;
use geojson::{JsonObject, JsonValue};
use geomedea::{Bounds, FeatureStream, LngLat, Properties, PropertyValue};
use wasm_bindgen::prelude::*;

#[cfg(feature = "log")]
#[macro_use]
extern crate log;
#[cfg(not(feature = "log"))]
#[macro_use]
mod log_stubs {
    macro_rules! debug {
        ($($x:tt)*) => {};
    }
}

#[wasm_bindgen]
pub fn setup_logging() {
    #[cfg(feature = "logging")]
    if let Err(e) = console_log::init_with_level(log::Level::Info) {
        println!("Error initializing logger: {:?}", e);
    }
    utils::set_panic_hook();
}

#[wasm_bindgen]
#[derive(Debug)]
pub struct HttpReader {
    url: String,
}

#[wasm_bindgen]
impl HttpReader {
    #[wasm_bindgen(constructor)]
    pub fn new(url: String) -> Self {
        Self { url }
    }

    async fn open(&mut self) -> Result<geomedea::HttpReader, JsError> {
        Ok(geomedea::HttpReader::open(&self.url).await?)
    }

    pub async fn select_all(&mut self) -> Result<JsValue, JsError> {
        debug!("selecting all");
        let mut http_reader = self.open().await?;
        debug!("http_reader: {:?}", http_reader);

        let feature_stream = http_reader.select_all().await?;
        debug!("opened iter");
        Ok(FeatureCollection::new(feature_stream)
            .await
            .map(JsValue::from)?)
    }

    pub async fn select_bbox(
        &mut self,
        top: f64,
        right: f64,
        bottom: f64,
        left: f64,
    ) -> Result<JsValue, JsError> {
        debug!("selecting bbox");
        let mut http_reader = self.open().await?;
        debug!("http_reader: {:?}", http_reader);

        let top_right = LngLat::degrees(right, top);
        let bottom_left = LngLat::degrees(left, bottom);
        let bounds = Bounds::from_corners(&top_right, &bottom_left);
        let feature_stream = http_reader.select_bbox(&bounds).await?;
        debug!("opened iter");
        Ok(FeatureCollection::new(feature_stream)
            .await
            .map(JsValue::from)?)
    }
}

struct FeatureCollection(geojson::FeatureCollection);
impl FeatureCollection {
    async fn new(mut feature_stream: FeatureStream) -> geomedea::Result<Self> {
        let mut geojson_feature_collection = geojson::FeatureCollection {
            bbox: None,
            features: vec![],
            foreign_members: None,
        };

        while let Some(feature) = feature_stream.next().await {
            let feature = feature?;
            geojson_feature_collection
                .features
                .push(geojson::Feature::from(GeoJsonFeature(feature)));
        }

        Ok(Self(geojson_feature_collection))
    }
}

impl From<FeatureCollection> for JsValue {
    fn from(value: FeatureCollection) -> Self {
        JsValue::from(value.0.to_string())
    }
}

struct GeoJsonFeature(geomedea::Feature);
impl From<GeoJsonFeature> for geojson::Feature {
    fn from(value: GeoJsonFeature) -> Self {
        let (geometry, properties) = value.0.into_inner();

        fn geomedea_properties_to_json(properties: Properties) -> JsonObject {
            fn property_to_json(property: PropertyValue) -> JsonValue {
                match property {
                    PropertyValue::Bool(value) => JsonValue::from(value),
                    PropertyValue::Int8(value) => JsonValue::from(value),
                    PropertyValue::UInt8(value) => JsonValue::from(value),
                    PropertyValue::Int16(value) => JsonValue::from(value),
                    PropertyValue::UInt16(value) => JsonValue::from(value),
                    PropertyValue::Int32(value) => JsonValue::from(value),
                    PropertyValue::UInt32(value) => JsonValue::from(value),
                    PropertyValue::Int64(value) => JsonValue::from(value),
                    PropertyValue::UInt64(value) => JsonValue::from(value),
                    PropertyValue::Float32(value) => JsonValue::from(value),
                    PropertyValue::Float64(value) => JsonValue::from(value),
                    PropertyValue::Bytes(value) => JsonValue::from(value),
                    PropertyValue::String(value) => JsonValue::from(value),
                    PropertyValue::Vec(value) => {
                        JsonValue::Array(value.into_iter().map(property_to_json).collect())
                    }
                    PropertyValue::Map(value) => {
                        JsonValue::from(geomedea_properties_to_json(value))
                    }
                }
            }
            let mut json_map = JsonObject::new();
            for (key, value) in properties.into_iter() {
                json_map.insert(key, property_to_json(value));
            }
            json_map
        }

        let geo_geometry: geo_types::Geometry = geometry.to_geo_type();
        let geojson_geometry: geojson::Geometry = (&geo_geometry).into();
        let geojson_properties = if properties.is_empty() {
            None
        } else {
            Some(geomedea_properties_to_json(properties))
        };

        geojson::Feature {
            bbox: None,
            geometry: Some(geojson_geometry),
            id: None,
            properties: geojson_properties,
            foreign_members: None,
        }
    }
}
