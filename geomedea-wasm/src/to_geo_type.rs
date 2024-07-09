use geomedea::{
    Geometry, GeometryCollection, LineString, LngLat, MultiLineString, MultiPoint, MultiPolygon,
    Polygon,
};

pub(crate) trait ToGeoType {
    type Output;
    fn to_geo_type(&self) -> Self::Output;
}

impl ToGeoType for Geometry {
    type Output = geo_types::Geometry;

    fn to_geo_type(&self) -> Self::Output {
        match self {
            Geometry::Point(point) => geo_types::Geometry::Point(point.to_geo_type()),
            Geometry::LineString(line_string) => {
                geo_types::Geometry::LineString(line_string.to_geo_type())
            }
            Geometry::Polygon(polygon) => geo_types::Geometry::Polygon(polygon.to_geo_type()),
            Geometry::MultiPoint(multi_point) => {
                geo_types::Geometry::MultiPoint(multi_point.to_geo_type())
            }
            Geometry::MultiLineString(multi_line_string) => {
                geo_types::Geometry::MultiLineString(multi_line_string.to_geo_type())
            }
            Geometry::MultiPolygon(multi_polygon) => {
                geo_types::Geometry::MultiPolygon(multi_polygon.to_geo_type())
            }
            Geometry::GeometryCollection(geometry_collection) => {
                geo_types::Geometry::GeometryCollection(geometry_collection.to_geo_type())
            }
        }
    }
}

impl ToGeoType for LngLat {
    type Output = geo_types::Point<f64>;

    fn to_geo_type(&self) -> Self::Output {
        geo_types::Point::new(self.lng_degrees(), self.lat_degrees())
    }
}

impl ToGeoType for LineString {
    type Output = geo_types::LineString<f64>;

    fn to_geo_type(&self) -> Self::Output {
        let points = self
            .points()
            .iter()
            .map(|point| point.to_geo_type().0)
            .collect();
        geo_types::LineString::new(points)
    }
}

impl ToGeoType for Polygon {
    type Output = geo_types::Polygon<f64>;

    fn to_geo_type(&self) -> Self::Output {
        let exterior = self
            .rings()
            .get(0)
            .unwrap_or(&LineString::new(vec![]))
            .to_geo_type();
        let interiors = self.rings()[1..]
            .iter()
            .map(|ring| ring.to_geo_type())
            .collect();
        geo_types::Polygon::new(exterior, interiors)
    }
}

impl ToGeoType for MultiPoint {
    type Output = geo_types::MultiPoint<f64>;

    fn to_geo_type(&self) -> Self::Output {
        let points: Vec<_> = self
            .points()
            .iter()
            .map(|point| point.to_geo_type())
            .collect();
        geo_types::MultiPoint(points)
    }
}

impl ToGeoType for MultiLineString {
    type Output = geo_types::MultiLineString<f64>;

    fn to_geo_type(&self) -> Self::Output {
        let line_strings: Vec<_> = self
            .line_strings()
            .iter()
            .map(|line_string| line_string.to_geo_type())
            .collect();
        geo_types::MultiLineString(line_strings)
    }
}

impl ToGeoType for MultiPolygon {
    type Output = geo_types::MultiPolygon<f64>;

    fn to_geo_type(&self) -> Self::Output {
        let polygons: Vec<_> = self
            .polygons()
            .iter()
            .map(|polygon| polygon.to_geo_type())
            .collect();
        geo_types::MultiPolygon(polygons)
    }
}

impl ToGeoType for GeometryCollection {
    type Output = geo_types::GeometryCollection<f64>;

    fn to_geo_type(&self) -> Self::Output {
        let geometries = self
            .geometries()
            .iter()
            .map(|geometry| geometry.to_geo_type())
            .collect();
        geo_types::GeometryCollection(geometries)
    }
}
