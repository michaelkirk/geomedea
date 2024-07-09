/// Creates a [`crate::geometry`] from a
/// [WKT](https://en.wikipedia.org/wiki/Well-known_text_representation_of_geometry) literal.
///
/// This is evaluated at compile time, so you don't need to worry about runtime errors from inavlid
/// WKT syntax.
///
/// Note that `POINT EMPTY` is not accepted because it is not representable as a `geo_types::Point`.
///
/// ```
/// use geomedea::wkt;
/// let point = wkt! { POINT(1.0 2.0) };
/// assert_eq!(point.lng_degrees(), 1.0);
/// assert_eq!(point.lat_degrees(), 2.0);
///
/// let geometry_collection = wkt! {
///     GEOMETRYCOLLECTION(
///         POINT(1.0 2.0),
///         LINESTRING EMPTY,
///         POLYGON((0.0 0.0,1.0 0.0,1.0 1.0,0.0 0.0))
///     )
/// };
/// assert_eq!(geometry_collection.geometries().len(), 3);
/// ```
#[macro_export]
macro_rules! wkt {
    // Hide distracting implementation details from the generated rustdoc.
    ($($wkt:tt)+) => {
        {
            $crate::wkt_internal!($($wkt)+)
        }
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! wkt_internal {
    (POINT EMPTY) => {
        compile_error!("EMPTY points are not supported in geo-types")
    };
    (POINT($x: literal, $y: literal)) => {
        compile_error!("There should not be a comma between x and y values in WKT.")
    };
    (POINT($x: literal $y: literal)) => {
        $crate::Point::degrees(f64::from($x), f64::from($y))
    };
    (POINT $($tail: tt)*) => {
        compile_error!("Invalid POINT wkt")
    };
    (LINESTRING EMPTY) => {
        $crate::LineString::new(vec![])
    };
    (LINESTRING ($($x: literal $y: literal),+)) => {
        $crate::LineString::new(vec![
            $($crate::Point::degrees(f64::from($x), f64::from($y)),)+
        ])
    };
    (LINESTRING ()) => {
        compile_error!("use `EMPTY` instead of () for an empty collection")
    };
    (LINESTRING $($tail: tt)*) => {
        compile_error!("Invalid LINESTRING wkt")
    };
    (POLYGON EMPTY) => {
        $crate::Polygon::new(vec![])
    };
    (POLYGON( $($rings_tt: tt),+ )) => {
        $crate::Polygon::new(vec![
           $($crate::wkt!(LINESTRING $rings_tt)),*
        ])
    };
    (POLYGON ()) => {
        compile_error!("use `EMPTY` instead of () for an empty collection")
    };
    (POLYGON $($tail: tt)*) => {
        compile_error!("Invalid POLYGON wkt")
    };
    (MULTIPOINT EMPTY) => {
        $crate::MultiPoint::new(vec![])
    };
    (MULTIPOINT ()) => {
        compile_error!("use `EMPTY` instead of () for an empty collection")
    };
    (MULTIPOINT ($($x: literal $y: literal),+ )) => {
        $crate::MultiPoint::new(vec![
            $($crate::Point::degrees(f64::from($x), f64::from($y)),)+
        ])
    };
    (MULTIPOINT $($tail: tt)*) => {
        compile_error!("Invalid MULTIPOINT wkt")
    };
    (MULTILINESTRING EMPTY) => {
        $crate::MultiLineString::new(vec![])
    };
    (MULTILINESTRING ()) => {
        compile_error!("use `EMPTY` instead of () for an empty collection")
    };
    (MULTILINESTRING ( $($line_string_tt: tt),+ )) => {
        $crate::MultiLineString::new(vec![
           $($crate::wkt!(LINESTRING $line_string_tt)),+
        ])
    };
    (MULTILINESTRING $($tail: tt)*) => {
        compile_error!("Invalid MULTILINESTRING wkt")
    };
    (MULTIPOLYGON EMPTY) => {
        $crate::MultiPolygon::new(vec![])
    };
    (MULTIPOLYGON ()) => {
        compile_error!("use `EMPTY` instead of () for an empty collection")
    };
    (MULTIPOLYGON ( $($polygon_tt: tt),+ )) => {
        $crate::MultiPolygon::new(vec![
           $($crate::wkt!(POLYGON $polygon_tt)),+
        ])
    };
    (MULTIPOLYGON $($tail: tt)*) => {
        compile_error!("Invalid MULTIPOLYGON wkt")
    };
    (RECT ($x1: literal $y1: literal, $x2: literal $y2: literal)) => {
        $crate::Bounds::from_corners(&wkt!(POINT($x1 $y1)), &wkt!(POINT($x2 $y2)))
    };
    (RECT $($tail: tt)*) => {
      compile_error!("Invalid RECT wkt. Should be like RECT(x1 y1,x2 y2)")
    };
    (GEOMETRYCOLLECTION EMPTY) => {
        $crate::GeometryCollection::new(vec![])
    };
    (GEOMETRYCOLLECTION ()) => {
        compile_error!("use `EMPTY` instead of () for an empty collection")
    };
    (GEOMETRYCOLLECTION ( $($el_type:tt $el_tt: tt),+ )) => {
        $crate::GeometryCollection::new(vec![
           $($crate::Geometry::from($crate::wkt!($el_type $el_tt))),+
        ])
    };
    (GEOMETRYCOLLECTION $($tail: tt)*) => {
        compile_error!("Invalid GEOMETRYCOLLECTION wkt")
    };
    ($name: ident ($($tail: tt)*)) => {
        compile_error!("Unknown type. Must be one of POINT, LINESTRING, POLYGON, MULTIPOINT, MULTILINESTRING, MULTIPOLYGON, RECT, or GEOMETRYCOLLECTION")
    };
}

#[cfg(test)]
mod tests {
    use crate::{Bounds, LngLat};
    use crate::{
        GeometryCollection, LineString, MultiLineString, MultiPoint, MultiPolygon, Polygon,
    };

    #[test]
    fn optional_decimal() {
        let point = wkt! { POINT(1 2.0) };
        assert_eq!(point, LngLat::degrees(1.0, 2.0));
    }

    #[test]
    fn point() {
        let point = wkt! { POINT(1 2) };
        assert_eq!(point, LngLat::degrees(1.0, 2.0));
    }

    #[test]
    fn line_string() {
        let line_string = wkt! { LINESTRING(1 2,3 4) };
        assert_eq!(
            line_string,
            LineString::new(vec![LngLat::degrees(1.0, 2.0), LngLat::degrees(3.0, 4.0)])
        );
    }

    #[test]
    fn polygon() {
        let polygon = wkt! { POLYGON((1 2,3 4)) };
        assert_eq!(
            polygon,
            Polygon::new(vec![LineString::new(vec![
                LngLat::degrees(1.0, 2.0),
                LngLat::degrees(3.0, 4.0)
            ])])
        );
    }

    #[test]
    fn multi_point() {
        let multi_point = wkt! { MULTIPOINT(1 2,3 4) };
        assert_eq!(
            multi_point,
            MultiPoint::new(vec![LngLat::degrees(1.0, 2.0), LngLat::degrees(3.0, 4.0),])
        )
    }

    #[test]
    fn multi_line_string() {
        let multi_line_string = wkt! { MULTILINESTRING((1 2,3 4),EMPTY,(5 6,7 8)) };
        assert_eq!(
            multi_line_string,
            MultiLineString::new(vec![
                LineString::new(vec![LngLat::degrees(1.0, 2.0), LngLat::degrees(3.0, 4.0),]),
                LineString::new(vec![]),
                LineString::new(vec![LngLat::degrees(5.0, 6.0), LngLat::degrees(7.0, 8.0),]),
            ])
        )
    }

    #[test]
    fn multi_polygon() {
        let multi_polygon = wkt! { MULTIPOLYGON(((0 1,1 1,1 0,0 1)),EMPTY,((5 5,7 8,5 6,5 5),(9 10, 11 12,12 12,9 10))) };
        assert_eq!(
            multi_polygon,
            MultiPolygon::new(vec![
                Polygon::new(vec![LineString::new(vec![
                    LngLat::degrees(0.0, 1.0),
                    LngLat::degrees(1.0, 1.0),
                    LngLat::degrees(1.0, 0.0),
                    LngLat::degrees(0.0, 1.0),
                ])]),
                Polygon::new(vec![]),
                Polygon::new(vec![
                    LineString::new(vec![
                        LngLat::degrees(5.0, 5.0),
                        LngLat::degrees(7.0, 8.0),
                        LngLat::degrees(5.0, 6.0),
                        LngLat::degrees(5.0, 5.0),
                    ]),
                    LineString::new(vec![
                        LngLat::degrees(9.0, 10.0),
                        LngLat::degrees(11.0, 12.0),
                        LngLat::degrees(12.0, 12.0),
                        LngLat::degrees(9.0, 10.0),
                    ]),
                ])
            ])
        )
    }

    #[test]
    fn geometry_collection() {
        let geometry_collection = wkt! { GEOMETRYCOLLECTION(
            POINT(1 2),
            LINESTRING(1 2,3 4),
            POLYGON((1 2,3 4,5 6,1 2)),
            MULTIPOINT(1 2,3 4),
            MULTILINESTRING((1 2,3 4),EMPTY,(5 6,7 8)),
            MULTIPOLYGON(((0 1,1 1,1 0,0 1)),EMPTY,((5 5,7 8,5 6,5 5),(9 10, 11 12,12 12,9 10)))
        )};
        let expected = GeometryCollection::new(vec![
            LngLat::degrees(1.0, 2.0).into(),
            LineString::new(vec![LngLat::degrees(1.0, 2.0), LngLat::degrees(3.0, 4.0)]).into(),
            Polygon::new(vec![LineString::new(vec![
                LngLat::degrees(1.0, 2.0),
                LngLat::degrees(3.0, 4.0),
                LngLat::degrees(5.0, 6.0),
                LngLat::degrees(1.0, 2.0),
            ])])
            .into(),
            MultiPoint::new(vec![LngLat::degrees(1.0, 2.0), LngLat::degrees(3.0, 4.0)]).into(),
            MultiLineString::new(vec![
                LineString::new(vec![LngLat::degrees(1.0, 2.0), LngLat::degrees(3.0, 4.0)]),
                LineString::new(vec![]),
                LineString::new(vec![LngLat::degrees(5.0, 6.0), LngLat::degrees(7.0, 8.0)]),
            ])
            .into(),
            MultiPolygon::new(vec![
                Polygon::new(vec![LineString::new(vec![
                    LngLat::degrees(0.0, 1.0),
                    LngLat::degrees(1.0, 1.0),
                    LngLat::degrees(1.0, 0.0),
                    LngLat::degrees(0.0, 1.0),
                ])]),
                Polygon::new(vec![]),
                Polygon::new(vec![
                    LineString::new(vec![
                        LngLat::degrees(5.0, 5.0),
                        LngLat::degrees(7.0, 8.0),
                        LngLat::degrees(5.0, 6.0),
                        LngLat::degrees(5.0, 5.0),
                    ]),
                    LineString::new(vec![
                        LngLat::degrees(9.0, 10.0),
                        LngLat::degrees(11.0, 12.0),
                        LngLat::degrees(12.0, 12.0),
                        LngLat::degrees(9.0, 10.0),
                    ]),
                ]),
            ])
            .into(),
        ]);
        assert_eq!(expected, geometry_collection)
    }

    #[test]
    fn bounds() {
        let bounds = wkt! { RECT(1 2,3 4) };
        assert_eq!(
            bounds,
            Bounds::from_corners(&LngLat::degrees(1.0, 2.0), &LngLat::degrees(3.0, 4.0))
        );
    }
}
