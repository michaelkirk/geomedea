use crate::feature::Properties;
use crate::{Feature, Geometry, LngLat, Writer};

pub fn small_pages(feature_count: usize, is_compressed: bool) -> Vec<u8> {
    let output = vec![];
    let mut writer = Writer::new(output, is_compressed).unwrap();
    // Use small page size to make sure we're testing multiple pages
    writer.set_page_size_goal(100);

    _points(feature_count, writer)
}

pub fn points(feature_count: usize, is_compressed: bool) -> Vec<u8> {
    let output = vec![];
    let writer = Writer::new(output, is_compressed).unwrap();
    _points(feature_count, writer)
}

pub fn _points(feature_count: usize, mut writer: Writer<Vec<u8>>) -> Vec<u8> {
    for feature_idx in 0..feature_count {
        let geometry = Geometry::from(LngLat::degrees(feature_idx as f64, feature_idx as f64));
        let mut properties = Properties::empty();
        properties.insert("name".to_string(), format!("prop-{}", feature_idx).into());
        let feature = Feature::new(geometry, properties);
        writer.add_feature(&feature).unwrap();
    }
    writer.finish().unwrap()
}
