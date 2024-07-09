use crate::Geometry;
use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::fmt::Formatter;

#[derive(Debug, Serialize, Deserialize)]
pub struct Feature {
    geometry: Geometry,
    properties: Properties,
}

impl Feature {
    pub fn new(geometry: Geometry, properties: Properties) -> Self {
        Self {
            geometry,
            properties,
        }
    }

    pub fn geometry(&self) -> &Geometry {
        &self.geometry
    }

    pub fn geometry_mut(&mut self) -> &mut Geometry {
        &mut self.geometry
    }

    pub fn properties(&self) -> &Properties {
        &self.properties
    }

    pub fn insert_property(&mut self, name: String, value: PropertyValue) {
        self.properties.insert(name, value);
    }

    pub fn property(&self, name: &str) -> Option<&PropertyValue> {
        self.properties.get(name)
    }

    pub fn into_inner(self) -> (Geometry, Properties) {
        (self.geometry, self.properties)
    }
}

type PropertyMap = HashMap<String, PropertyValue>;
#[derive(Clone, PartialEq)]
pub struct Properties {
    ordered_keys: Vec<String>,
    property_map: PropertyMap,
}

impl Serialize for Properties {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq_serializer = serializer.serialize_seq(Some(self.ordered_keys.len()))?;
        for (key, value) in self.iter() {
            seq_serializer.serialize_element(&(key, value))?;
        }
        seq_serializer.end()
    }
}

impl<'de> Deserialize<'de> for Properties {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let ordered_values: Vec<(String, PropertyValue)> = Vec::deserialize(deserializer)?;

        let mut property_map = PropertyMap::new();
        let mut ordered_keys = vec![];
        for (key, value) in ordered_values.into_iter() {
            ordered_keys.push(key.clone());
            property_map.insert(key, value);
        }
        Ok(Self {
            ordered_keys,
            property_map,
        })
    }
}

impl std::fmt::Debug for Properties {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.is_empty() {
            write!(f, "Properties {{ }}")
        } else {
            let mut debug_struct = f.debug_struct("Properties");
            for (key, value) in self.iter() {
                debug_struct.field(key, value);
            }
            debug_struct.finish()
        }
    }
}

impl Properties {
    pub fn empty() -> Self {
        Self {
            ordered_keys: vec![],
            property_map: PropertyMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.ordered_keys.is_empty()
    }

    pub fn insert(&mut self, name: String, value: PropertyValue) -> Option<PropertyValue> {
        assert!(
            !self.ordered_keys.contains(&name),
            "handle caller error of repeated property"
        );
        self.ordered_keys.push(name.clone());
        self.property_map.insert(name, value)
    }

    pub fn get(&self, name: &str) -> Option<&PropertyValue> {
        self.property_map.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &PropertyValue)> {
        PropertyIter {
            keys_iter: self.ordered_keys.iter(),
            values: &self.property_map,
        }
    }
}

impl IntoIterator for Properties {
    type Item = (String, PropertyValue);
    type IntoIter = PropertiesIntoIter;

    fn into_iter(self) -> Self::IntoIter {
        Self::IntoIter {
            keys_iter: self.ordered_keys.into_iter(),
            property_map: self.property_map,
        }
    }
}

pub struct PropertiesIntoIter {
    keys_iter: std::vec::IntoIter<String>,
    property_map: PropertyMap,
}

impl Iterator for PropertiesIntoIter {
    type Item = (String, PropertyValue);

    fn next(&mut self) -> Option<(String, PropertyValue)> {
        let next_key = self.keys_iter.next()?;
        let next_value = self
            .property_map
            .remove(&next_key)
            .expect("value for each key");
        Some((next_key, next_value))
    }
}

struct PropertyIter<'a> {
    keys_iter: std::slice::Iter<'a, String>,
    values: &'a PropertyMap,
}

impl<'a> Iterator for PropertyIter<'a> {
    type Item = (&'a str, &'a PropertyValue);

    fn next(&mut self) -> Option<Self::Item> {
        let next_key = self.keys_iter.next()?;
        let Some(next_value) = self.values.get(next_key) else {
            todo!("handle missing value");
        };
        Some((next_key, next_value))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PropertyValue {
    Bool(bool),
    Int8(i8),
    UInt8(u8),
    Int16(i16),
    UInt16(u16),
    Int32(i32),
    UInt32(u32),
    Int64(i64),
    UInt64(u64),
    Float32(f32),
    Float64(f64),
    Bytes(Vec<u8>),
    String(String),
    Vec(Vec<PropertyValue>),
    Map(Properties),
}

impl From<&str> for PropertyValue {
    fn from(value: &str) -> Self {
        PropertyValue::String(value.to_string())
    }
}

impl From<String> for PropertyValue {
    fn from(value: String) -> Self {
        PropertyValue::String(value)
    }
}
