use crate::geometry::LatLon;

const EARTH_RADIUS_NM: f64 = 3440.065;
const MAX_DISPLAY_SEGMENT_NM: f64 = 120.0;
const MAX_DISPLAY_PATH_POINTS: usize = 128;

pub fn great_circle_distance_nm(from: LatLon, to: LatLon) -> f64 {
    let dlat = (to.lat - from.lat).to_radians();
    let dlon = (to.lon - from.lon).to_radians();
    let lat1 = from.lat.to_radians();
    let lat2 = to.lat.to_radians();
    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_NM * h.sqrt().asin()
}

pub fn initial_course_deg(from: LatLon, to: LatLon) -> f64 {
    let from_lat = from.lat.to_radians();
    let from_lon = from.lon.to_radians();
    let to_lat = to.lat.to_radians();
    let to_lon = to.lon.to_radians();
    let delta_lon = to_lon - from_lon;
    let y = delta_lon.sin() * to_lat.cos();
    let x = from_lat.cos() * to_lat.sin() - from_lat.sin() * to_lat.cos() * delta_lon.cos();
    normalize_degrees(y.atan2(x).to_degrees())
}

pub fn cross_track_left_nm(from: LatLon, to: LatLon, position: LatLon) -> f64 {
    let distance13 = great_circle_distance_nm(from, position) / EARTH_RADIUS_NM;
    if distance13 <= f64::EPSILON {
        return 0.0;
    }
    let course13 = initial_course_deg(from, position).to_radians();
    let course12 = initial_course_deg(from, to).to_radians();
    -((distance13.sin() * (course13 - course12).sin()).asin() * EARTH_RADIUS_NM)
}

pub fn great_circle_display_path(from: LatLon, to: LatLon) -> Vec<LatLon> {
    let distance_nm = great_circle_distance_nm(from, to);
    if distance_nm <= f64::EPSILON {
        return vec![from, to];
    }
    let segment_count = ((distance_nm / MAX_DISPLAY_SEGMENT_NM).ceil() as usize)
        .clamp(1, MAX_DISPLAY_PATH_POINTS - 1);
    (0..=segment_count)
        .map(|index| great_circle_intermediate(from, to, index as f64 / segment_count as f64))
        .collect()
}

fn great_circle_intermediate(from: LatLon, to: LatLon, fraction: f64) -> LatLon {
    if fraction <= 0.0 {
        return from;
    }
    if fraction >= 1.0 {
        return to;
    }

    let lat1 = from.lat.to_radians();
    let lon1 = from.lon.to_radians();
    let lat2 = to.lat.to_radians();
    let lon2 = to.lon.to_radians();
    let angular_distance = great_circle_distance_nm(from, to) / EARTH_RADIUS_NM;
    if angular_distance <= f64::EPSILON {
        return from;
    }

    let sin_delta = angular_distance.sin();
    let a = ((1.0 - fraction) * angular_distance).sin() / sin_delta;
    let b = (fraction * angular_distance).sin() / sin_delta;
    let x = a * lat1.cos() * lon1.cos() + b * lat2.cos() * lon2.cos();
    let y = a * lat1.cos() * lon1.sin() + b * lat2.cos() * lon2.sin();
    let z = a * lat1.sin() + b * lat2.sin();
    let lat = z.atan2((x * x + y * y).sqrt()).to_degrees();
    let lon = y.atan2(x).to_degrees();
    LatLon { lat, lon }
}

fn normalize_degrees(value: f64) -> f64 {
    value.rem_euclid(360.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cross_track_is_positive_left_of_course() {
        let from = LatLon { lat: 0.0, lon: 0.0 };
        let to = LatLon { lat: 0.0, lon: 1.0 };
        let left = LatLon { lat: 0.1, lon: 0.5 };
        let right = LatLon {
            lat: -0.1,
            lon: 0.5,
        };
        assert!(cross_track_left_nm(from, to, left) > 0.0);
        assert!(cross_track_left_nm(from, to, right) < 0.0);
    }

    #[test]
    fn display_path_densifies_long_legs() {
        let from = LatLon {
            lat: 37.62,
            lon: -122.38,
        };
        let to = LatLon {
            lat: 35.55,
            lon: 139.78,
        };
        let path = great_circle_display_path(from, to);
        assert!(path.len() > 10);
        assert_eq!(path.first(), Some(&from));
        assert_eq!(path.last(), Some(&to));
    }
}
