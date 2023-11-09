use crate::{Bounds, LngLat};

/// REVIEW: Why does HILBERT_MAX exist and why is it half the bit width?
/// Since the hilbert impl is muxing all the bits, it seems like we're throwing away precision.
const HILBERT_MAX: u32 = u16::MAX as u32;

// Based on public domain code at https://github.com/rawrunprotected/hilbert_curves
fn hilbert(x: u32, y: u32) -> u32 {
    debug_assert!(x <= HILBERT_MAX);
    debug_assert!(y <= HILBERT_MAX);

    let mut a = x ^ y;
    let mut b = 0xFFFF ^ a;
    let mut c = 0xFFFF ^ (x | y);
    let mut d = x & (y ^ 0xFFFF);

    let mut aa = a | (b >> 1);
    let mut bb = (a >> 1) ^ a;
    let mut cc = ((c >> 1) ^ (b & (d >> 1))) ^ c;
    let mut dd = ((a & (c >> 1)) ^ (d >> 1)) ^ d;

    a = aa;
    b = bb;
    c = cc;
    d = dd;
    aa = (a & (a >> 2)) ^ (b & (b >> 2));
    bb = (a & (b >> 2)) ^ (b & ((a ^ b) >> 2));
    cc ^= (a & (c >> 2)) ^ (b & (d >> 2));
    dd ^= (b & (c >> 2)) ^ ((a ^ b) & (d >> 2));

    a = aa;
    b = bb;
    c = cc;
    d = dd;
    aa = (a & (a >> 4)) ^ (b & (b >> 4));
    bb = (a & (b >> 4)) ^ (b & ((a ^ b) >> 4));
    cc ^= (a & (c >> 4)) ^ (b & (d >> 4));
    dd ^= (b & (c >> 4)) ^ ((a ^ b) & (d >> 4));

    a = aa;
    b = bb;
    c = cc;
    d = dd;
    cc ^= (a & (c >> 8)) ^ (b & (d >> 8));
    dd ^= (b & (c >> 8)) ^ ((a ^ b) & (d >> 8));

    a = cc ^ (cc >> 1);
    b = dd ^ (dd >> 1);

    let mut i0 = x ^ y;
    let mut i1 = b | (0xFFFF ^ (i0 | a));

    i0 = (i0 | (i0 << 8)) & 0x00FF00FF;
    i0 = (i0 | (i0 << 4)) & 0x0F0F0F0F;
    i0 = (i0 | (i0 << 2)) & 0x33333333;
    i0 = (i0 | (i0 << 1)) & 0x55555555;

    i1 = (i1 | (i1 << 8)) & 0x00FF00FF;
    i1 = (i1 | (i1 << 4)) & 0x0F0F0F0F;
    i1 = (i1 | (i1 << 2)) & 0x33333333;
    i1 = (i1 | (i1 << 1)) & 0x55555555;

    (i1 << 1) | i0
}

/// Project a point to a hilbert curve that fills extent.
///
/// The point is scales to 0..HILBERT_MAX relative to `extent`
/// `extent.min() corresponds to (0,0) and extent.max() corresponds to (HILBER_MAX, HILBERT_MAX)
pub(crate) fn scaled_hilbert(point: &LngLat, extent: &Bounds) -> u32 {
    let x = (point.lng_unscaled() as i64 - extent.min().lng_unscaled() as i64) as u64
        * HILBERT_MAX as u64
        / extent.unscaled_lng_width() as u64;
    let y = (point.lat_unscaled() as i64 - extent.min().lat_unscaled() as i64) as u64
        * HILBERT_MAX as u64
        / extent.unscaled_lat_height() as u64;
    hilbert(x as u32, y as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wkt;

    #[test]
    fn check_scaled_hilbert() {
        let node_1 = wkt!(RECT(0 0,1 1));
        let node_2 = wkt!(RECT(2 2,3 3));
        let nodes = vec![node_1, node_2];

        let mut extent = Bounds::empty();
        for node in &nodes {
            extent.extend(node);
        }

        assert_eq!(143165576, scaled_hilbert(&nodes[0].center(), &extent));
        assert_eq!(2720145952, scaled_hilbert(&nodes[1].center(), &extent));
    }
}
