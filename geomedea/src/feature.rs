use crate::Geometry;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
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
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Properties {
    // TODO: implement sorting
    keys: Vec<String>,
    values: BTreeMap<String, PropertyValue>,
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
        Properties {
            keys: vec![],
            values: BTreeMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn insert(&mut self, name: String, value: PropertyValue) -> Option<PropertyValue> {
        assert!(
            !self.keys.contains(&name),
            "handle caller error of repeated property"
        );
        self.keys.push(name.clone());
        self.values.insert(name, value)
    }

    pub fn get(&self, name: &str) -> Option<&PropertyValue> {
        self.values.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &PropertyValue)> {
        PropertyIter {
            keys_iter: self.keys.iter(),
            values: &self.values,
        }
    }
}

struct PropertyIter<'a> {
    keys_iter: std::slice::Iter<'a, String>,
    values: &'a BTreeMap<String, PropertyValue>,
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
    Map(HashMap<String, PropertyValue>),
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
