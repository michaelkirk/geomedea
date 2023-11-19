use crate::geometry::*;

use approx::{AbsDiffEq, RelativeEq};

impl AbsDiffEq<Self> for Geometry {
    type Epsilon = f64;

    fn default_epsilon() -> Self::Epsilon {
        Self::Epsilon::EPSILON
    }

    fn abs_diff_eq(&self, other: &Self, epsilon: Self::Epsilon) -> bool {
        match (self, other) {
            (Geometry::Point(a), Geometry::Point(b)) => a.abs_diff_eq(b, epsilon),
            _ => todo!(),
        }
    }
}

impl RelativeEq for Geometry {
    fn default_max_relative() -> Self::Epsilon {
        Self::Epsilon::EPSILON
    }

    fn relative_eq(
        &self,
        other: &Self,
        epsilon: Self::Epsilon,
        max_relative: Self::Epsilon,
    ) -> bool {
        match (self, other) {
            (Geometry::Point(a), Geometry::Point(b)) => a.relative_eq(b, epsilon, max_relative),
            _ => todo!(),
        }
    }
}

impl AbsDiffEq<Self> for LngLat {
    type Epsilon = f64;

    fn default_epsilon() -> Self::Epsilon {
        Self::Epsilon::EPSILON
    }

    fn abs_diff_eq(&self, other: &Self, epsilon: Self::Epsilon) -> bool {
        self.lng_degrees()
            .abs_diff_eq(&other.lng_degrees(), epsilon)
            && self
                .lat_degrees()
                .abs_diff_eq(&other.lat_degrees(), epsilon)
    }
}

impl RelativeEq for LngLat {
    fn default_max_relative() -> Self::Epsilon {
        Self::Epsilon::EPSILON
    }

    fn relative_eq(
        &self,
        other: &Self,
        epsilon: Self::Epsilon,
        max_relative: Self::Epsilon,
    ) -> bool {
        self.lng_degrees()
            .relative_eq(&other.lng_degrees(), epsilon, max_relative)
            && self
                .lat_degrees()
                .relative_eq(&other.lat_degrees(), epsilon, max_relative)
    }
}
