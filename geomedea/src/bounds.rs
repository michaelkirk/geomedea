use crate::LngLat;
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Formatter};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bounds {
    min: LngLat,
    max: LngLat,
}

impl Debug for Bounds {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "RECT({} {}, {} {})",
            self.min.lng_degrees(),
            self.min.lat_degrees(),
            self.max.lng_degrees(),
            self.max.lat_degrees()
        )
    }
}

impl Bounds {
    pub fn min(&self) -> &LngLat {
        &self.min
    }

    pub fn max(&self) -> &LngLat {
        &self.max
    }

    pub fn unscaled_lng_width(&self) -> u32 {
        (self.max.lng_unscaled() as i64 - self.min.lng_unscaled() as i64) as u32
    }

    pub fn unscaled_lat_height(&self) -> u32 {
        (self.max.lat_unscaled() as i64 - self.min.lat_unscaled() as i64) as u32
    }

    pub(crate) fn empty() -> Self {
        Bounds {
            min: LngLat::unscaled(i32::MAX, i32::MAX),
            max: LngLat::unscaled(i32::MIN, i32::MIN),
        }
    }

    pub fn from_corners(a: &LngLat, b: &LngLat) -> Self {
        // TODO: should I not do this sorting in order to allow bounds to span IDL?
        let min_lng = a.lng_unscaled().min(b.lng_unscaled());
        let min_lat = a.lat_unscaled().min(b.lat_unscaled());
        let max_lng = a.lng_unscaled().max(b.lng_unscaled());
        let max_lat = a.lat_unscaled().max(b.lat_unscaled());
        Bounds {
            min: LngLat::unscaled(min_lng, min_lat),
            max: LngLat::unscaled(max_lng, max_lat),
        }
    }

    pub fn extend(&mut self, other: &Bounds) {
        if other.max.lng_unscaled() > self.max.lng_unscaled() {
            self.max.set_lng_unscaled(other.max.lng_unscaled());
        }
        if other.max.lat_unscaled() > self.max.lat_unscaled() {
            self.max.set_lat_unscaled(other.max.lat_unscaled());
        }
        if other.min.lng_unscaled() < self.min.lng_unscaled() {
            self.min.set_lng_unscaled(other.min.lng_unscaled());
        }
        if other.min.lat_unscaled() < self.min.lat_unscaled() {
            self.min.set_lat_unscaled(other.min.lat_unscaled());
        }
    }

    pub fn extend_point(&mut self, point: &LngLat) {
        if point.lng_unscaled() > self.max.lng_unscaled() {
            self.max.set_lng_unscaled(point.lng_unscaled());
        }
        if point.lat_unscaled() > self.max.lat_unscaled() {
            self.max.set_lat_unscaled(point.lat_unscaled());
        }
        if point.lng_unscaled() < self.min.lng_unscaled() {
            self.min.set_lng_unscaled(point.lng_unscaled());
        }
        if point.lat_unscaled() < self.min.lat_unscaled() {
            self.min.set_lat_unscaled(point.lat_unscaled());
        }
    }

    pub(crate) fn center(&self) -> LngLat {
        let half_lng_width = self.unscaled_lng_width() / 2;
        let half_lat_height = self.unscaled_lat_height() / 2;

        let mid_lng = self.min.lng_unscaled() + half_lng_width as i32;
        let mid_lat = self.min.lat_unscaled() + half_lat_height as i32;

        LngLat::unscaled(mid_lng, mid_lat)
    }

    pub(crate) fn intersects(&self, other: &Bounds) -> bool {
        if self.max.lng_unscaled() < other.min.lng_unscaled() {
            return false;
        }

        if self.max.lat_unscaled() < other.min.lat_unscaled() {
            return false;
        }

        if self.min.lng_unscaled() > other.max.lng_unscaled() {
            return false;
        }

        if self.min.lat_unscaled() > other.max.lat_unscaled() {
            return false;
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use crate::wkt;

    #[test]
    fn center() {
        let bounds = wkt!(RECT(0 0,3 3));
        assert_eq!(wkt!(POINT(1.5 1.5)), bounds.center());

        let node_2 = wkt!(RECT(2 2,3 3));
        assert_eq!(wkt!(POINT(2.5 2.5)), node_2.center());
    }

    #[test]
    fn negative() {
        let bounds = wkt!(RECT(1 2,-3 -6));
        assert_eq!(wkt!(POINT(-1.0 - 2.0)), bounds.center());
    }
}
