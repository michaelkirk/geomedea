use crate::{
    Bounds, Geometry, LineString, LngLat, MultiLineString, MultiPoint, MultiPolygon, Polygon,
};

pub(crate) trait Bounded {
    fn extend_bounds(&self, bounds: &mut Bounds);
    fn bounds(&self) -> Bounds {
        let mut bounds = Bounds::empty();
        self.extend_bounds(&mut bounds);
        bounds
    }
}

impl Bounded for LngLat {
    fn extend_bounds(&self, bounds: &mut Bounds) {
        bounds.extend_point(self)
    }
}

impl Bounded for LineString {
    fn extend_bounds(&self, bounds: &mut Bounds) {
        for point in &self.0 {
            point.extend_bounds(bounds)
        }
    }
}

impl Bounded for Polygon {
    fn extend_bounds(&self, bounds: &mut Bounds) {
        for line_string in &self.0 {
            line_string.extend_bounds(bounds)
        }
    }
}

impl Bounded for MultiPoint {
    fn extend_bounds(&self, bounds: &mut Bounds) {
        for point in &self.0 {
            point.extend_bounds(bounds)
        }
    }
}

impl Bounded for MultiLineString {
    fn extend_bounds(&self, bounds: &mut Bounds) {
        for line_string in &self.0 {
            line_string.extend_bounds(bounds)
        }
    }
}

impl Bounded for MultiPolygon {
    fn extend_bounds(&self, bounds: &mut Bounds) {
        for polygon in &self.0 {
            polygon.extend_bounds(bounds)
        }
    }
}

impl Bounded for Geometry {
    fn extend_bounds(&self, bounds: &mut Bounds) {
        match self {
            Geometry::Point(point) => point.extend_bounds(bounds),
            Geometry::LineString(line_string) => line_string.extend_bounds(bounds),
            Geometry::Polygon(polygon) => polygon.extend_bounds(bounds),
            Geometry::MultiPoint(multi_point) => multi_point.extend_bounds(bounds),
            Geometry::MultiLineString(multi_line_string) => multi_line_string.extend_bounds(bounds),
            Geometry::MultiPolygon(multi_polygon) => multi_polygon.extend_bounds(bounds),
            Geometry::GeometryCollection(geometries) => {
                for geometry in &geometries.0 {
                    Bounded::extend_bounds(geometry, bounds)
                }
            }
        }
    }
}
