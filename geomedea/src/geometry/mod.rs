mod bounded;
#[cfg(test)]
mod relative_eq;

use crate::bounds::Bounds;
pub(crate) use bounded::Bounded;
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Formatter};

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Geometry {
    Point(LngLat),
    LineString(LineString),
    Polygon(Polygon),
    MultiPoint(MultiPoint),
    MultiLineString(MultiLineString),
    MultiPolygon(MultiPolygon),
    GeometryCollection(GeometryCollection),
}

impl Debug for Geometry {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Geometry::Point(g) => write!(f, "POINT({g:?})"),
            Geometry::LineString(g) => g.fmt(f),
            Geometry::Polygon(g) => g.fmt(f),
            Geometry::MultiPoint(g) => g.fmt(f),
            Geometry::MultiLineString(g) => g.fmt(f),
            Geometry::MultiPolygon(g) => g.fmt(f),
            Geometry::GeometryCollection(g) => g.fmt(f),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LngLat {
    lng: i32,
    lat: i32,
}
pub type Point = LngLat;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LineString(Vec<LngLat>);
impl LineString {
    pub fn new(points: Vec<LngLat>) -> Self {
        Self(points)
    }
    pub fn points_len(&self) -> usize {
        self.0.len()
    }
    pub fn points(&self) -> &[LngLat] {
        &self.0
    }
    pub fn push_point(&mut self, point: LngLat) {
        self.0.push(point)
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Polygon(Vec<LineString>);
impl Polygon {
    pub fn new(rings: Vec<LineString>) -> Self {
        Self(rings)
    }

    pub fn rings(&self) -> &[LineString] {
        &self.0
    }

    pub fn push_ring(&mut self, ring: LineString) {
        self.0.push(ring)
    }

    pub fn rings_mut(&mut self) -> &mut [LineString] {
        &mut self.0
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MultiPoint(Vec<Point>);
impl MultiPoint {
    pub fn new(points: Vec<Point>) -> Self {
        Self(points)
    }
    pub fn points(&self) -> &[Point] {
        &self.0
    }
    pub fn push(&mut self, point: Point) {
        self.0.push(point)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MultiLineString(Vec<LineString>);
impl MultiLineString {
    pub fn new(line_strings: Vec<LineString>) -> Self {
        Self(line_strings)
    }
    pub fn push(&mut self, line_string: LineString) {
        self.0.push(line_string)
    }

    pub fn line_strings(&self) -> &[LineString] {
        &self.0
    }

    pub fn first(&mut self) -> Option<&LineString> {
        self.0.first()
    }

    pub fn first_mut(&mut self) -> Option<&mut LineString> {
        self.0.first_mut()
    }

    pub fn last(&mut self) -> Option<&LineString> {
        self.0.last()
    }

    pub fn last_mut(&mut self) -> Option<&mut LineString> {
        self.0.last_mut()
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MultiPolygon(Vec<Polygon>);
impl MultiPolygon {
    pub fn new(polygons: Vec<Polygon>) -> Self {
        Self(polygons)
    }
    pub fn push(&mut self, polygon: Polygon) {
        self.0.push(polygon)
    }
    pub fn polygons(&self) -> &[Polygon] {
        &self.0
    }
    pub fn polygons_mut(&mut self) -> &mut [Polygon] {
        &mut self.0
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeometryCollection(Vec<Geometry>);
impl GeometryCollection {
    pub fn new(geometries: Vec<Geometry>) -> Self {
        Self(geometries)
    }

    pub fn push(&mut self, geometry: Geometry) {
        self.0.push(geometry)
    }

    pub fn pop(&mut self) -> Option<Geometry> {
        self.0.pop()
    }

    pub fn geometries(&self) -> &[Geometry] {
        &self.0
    }

    pub fn geometries_mut(&mut self) -> &mut [Geometry] {
        &mut self.0
    }

    pub fn bounds(&self) -> Bounds {
        let mut bounds = Bounds::empty();
        for geometry in &self.0 {
            bounds.extend(&geometry.bounds())
        }
        bounds
    }
}

impl From<Point> for Geometry {
    fn from(value: Point) -> Self {
        Self::Point(value)
    }
}
impl From<LineString> for Geometry {
    fn from(value: LineString) -> Self {
        Self::LineString(value)
    }
}
impl From<Polygon> for Geometry {
    fn from(value: Polygon) -> Self {
        Self::Polygon(value)
    }
}
impl From<MultiPoint> for Geometry {
    fn from(value: MultiPoint) -> Self {
        Self::MultiPoint(value)
    }
}
impl From<MultiLineString> for Geometry {
    fn from(value: MultiLineString) -> Self {
        Self::MultiLineString(value)
    }
}
impl From<MultiPolygon> for Geometry {
    fn from(value: MultiPolygon) -> Self {
        Self::MultiPolygon(value)
    }
}
impl From<GeometryCollection> for Geometry {
    fn from(value: GeometryCollection) -> Self {
        Self::GeometryCollection(value)
    }
}

impl Debug for LngLat {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.lng_degrees(), self.lat_degrees())
    }
}

impl Debug for LineString {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "LINESTRING ")?;
        fmt_points(&self.0, f)
    }
}

fn fmt_points(points: &[LngLat], f: &mut Formatter) -> std::fmt::Result {
    if points.is_empty() {
        return write!(f, "EMPTY");
    }

    write!(f, "(")?;
    for (idx, point) in points.iter().enumerate() {
        if idx == points.len() - 1 {
            write!(f, "{point:?}")?;
        } else {
            write!(f, "{point:?},")?;
        }
    }
    write!(f, ")")
}

impl Debug for Polygon {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "POLYGON ")?;
        fmt_polygon_rings_wkt(self, f)
    }
}

fn fmt_polygon_rings_wkt(polygon: &Polygon, f: &mut Formatter) -> std::fmt::Result {
    if polygon.rings().is_empty() {
        return write!(f, "EMPTY");
    }

    write!(f, "(")?;
    for (idx, ring) in polygon.rings().iter().enumerate() {
        if idx == polygon.rings().len() - 1 {
            fmt_points(ring.points(), f)?;
        } else {
            fmt_points(ring.points(), f)?;
            write!(f, ",")?;
        }
    }
    write!(f, ")")
}

impl Debug for MultiPolygon {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.polygons().is_empty() {
            write!(f, "MULTIPOLYGON EMPTY")?;
        } else {
            write!(f, "MULTIPOLYGON (")?;
            for (idx, polygon) in self.polygons().iter().enumerate() {
                if idx == self.polygons().len() - 1 {
                    fmt_polygon_rings_wkt(polygon, f)?;
                } else {
                    fmt_polygon_rings_wkt(polygon, f)?;
                    write!(f, ",")?;
                }
            }
            write!(f, ")")?;
        }
        Ok(())
    }
}

pub const COORD_PRECISION_NANOS: i32 = 100;

/// Number of internal units (as returned from [`Lat::inner`]) in one degree.
///
/// See [`COORD_PRECISION_NANOS`].
pub const COORD_SCALE_FACTOR: f64 = (1_000_000_000 / COORD_PRECISION_NANOS) as f64;

impl LngLat {
    #[inline]
    fn to_degrees(unscaled_value: i32) -> f64 {
        unscaled_value as f64 / COORD_SCALE_FACTOR
    }

    fn to_unscaled(degrees: f64) -> i32 {
        (degrees * COORD_SCALE_FACTOR) as i32
    }

    pub fn unscaled(lng: i32, lat: i32) -> Self {
        Self { lng, lat }
    }

    pub fn degrees(lng: f64, lat: f64) -> Self {
        Self {
            lng: Self::to_unscaled(lng),
            lat: Self::to_unscaled(lat),
        }
    }

    pub fn lng_unscaled(&self) -> i32 {
        self.lng
    }

    pub fn lat_unscaled(&self) -> i32 {
        self.lat
    }

    pub fn set_lng_unscaled(&mut self, unscaled_value: i32) {
        self.lng = unscaled_value;
    }

    pub fn set_lat_unscaled(&mut self, unscaled_value: i32) {
        self.lat = unscaled_value;
    }

    pub fn lng_degrees(&self) -> f64 {
        Self::to_degrees(self.lng)
    }

    pub fn lat_degrees(&self) -> f64 {
        Self::to_degrees(self.lat)
    }

    // This is potentially lossy.
    pub fn set_lng_degrees(&mut self, degrees: f64) {
        self.lng = Self::to_unscaled(degrees)
    }

    // This is potentially lossy.
    pub fn set_lat_degrees(&mut self, degrees: f64) {
        self.lat = Self::to_unscaled(degrees)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wkt;

    #[test]
    fn coordinate_scaling() {
        let coord = LngLat::degrees(-118.2562, 34.1060);
        assert_eq!(coord.lng_unscaled(), -1182562000);
        assert_eq!(coord.lat_unscaled(), 341060000);
    }

    #[test]
    fn bounds_checking() {
        let collection = wkt!(GEOMETRYCOLLECTION(
            POINT(1 2),
            LINESTRING(0 0,1 1),
            POLYGON((-1 -1,-1 0,0 0,-1 -1)),
            MULTIPOINT(10 0,1 1),
            MULTILINESTRING((10 0,1 1),(0 20,0 0)),
            MULTIPOLYGON(((0 0,1 1,0 1,0 0)),((-20 0,-20 5,0 0,-20 0))),
            GEOMETRYCOLLECTION(POINT(1 -30))
        ));
        let expected = wkt!(RECT(-20 -30,10 20));

        assert_eq!(expected, collection.bounds())
    }
}
