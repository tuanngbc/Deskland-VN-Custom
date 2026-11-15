//! Solar direction from UTC and the selected coordinates, without network calls.
//! Solar coordinates: https://aa.usno.navy.mil/faq/sun_approx
#[derive(Clone, Copy, Debug)]
pub struct Sun {
    pub east: f32,
    pub north: f32,
    pub up: f32,
}

pub fn position(unix_seconds: f64, latitude: f64, longitude: f64) -> Sun {
    let d = unix_seconds / 86400.0 + 2440587.5 - 2451545.0;
    let g = (357.529 + 0.98560028 * d).rem_euclid(360.0).to_radians();
    let q = (280.459 + 0.98564736 * d).rem_euclid(360.0);
    let l = (q + 1.915 * g.sin() + 0.020 * (2.0*g).sin()).to_radians();
    let e = (23.439 - 0.00000036 * d).to_radians();
    let ra = (e.cos() * l.sin()).atan2(l.cos());
    let dec = (e.sin() * l.sin()).asin();
    let sidereal = (280.46061837 + 360.98564736629 * d + longitude).rem_euclid(360.0).to_radians();
    let hour_angle = sidereal - ra;
    let lat = latitude.to_radians();
    Sun {
        east: (-dec.cos()*hour_angle.sin()) as f32,
        north: (dec.sin()*lat.cos()-dec.cos()*hour_angle.cos()*lat.sin()) as f32,
        up: (dec.sin()*lat.sin()+dec.cos()*hour_angle.cos()*lat.cos()) as f32,
    }
}

/// Clock-only fallback when no coordinates have been selected; noon peaks at 12.
pub fn clock(phase: f32) -> Sun {
    let a = (phase-0.25)*std::f32::consts::TAU;
    Sun { east: a.cos(), north: 0.35, up: a.sin() }
}

/// Ground displacement per unit of tree height. East = screen right, north = up.
pub fn shadow(sun: Sun) -> (f32, f32, f32) {
    if sun.up <= 0.0 { return (0.0, 0.0, 0.0); }
    let denom = sun.up.max(0.18);
    (-sun.east/denom, sun.north/denom*0.45,
     (sun.up/0.15).clamp(0.0, 1.0)*0.34)
}

#[cfg(test)]
mod tests {
    use super::*;
    // 2026-11-15 00:00 UTC. Biên Hòa uses UTC+7.
    const DAY: f64 = 1794700800.0;

    #[test]
    fn bien_hoa_morning_noon_evening_and_night() {
        let morning = position(DAY+3600.0, 10.94469, 106.82432); // 08:00
        let noon = position(DAY+5.0*3600.0, 10.94469, 106.82432);
        let evening = position(DAY+10.0*3600.0, 10.94469, 106.82432);
        let night = position(DAY+17.0*3600.0, 10.94469, 106.82432);
        assert!(morning.east > 0.0 && evening.east < 0.0);
        assert!(noon.up > morning.up && noon.up > evening.up);
        assert!(shadow(morning).0 < 0.0 && shadow(evening).0 > 0.0);
        assert!(shadow(noon).0.abs() < shadow(morning).0.abs());
        assert_eq!(shadow(night).2, 0.0);
    }

    #[test]
    fn longitude_and_season_change_solar_direction() {
        let north_winter = position(DAY+12.0*3600.0, 50.0, 0.0);
        let north_summer = position(1782043200.0, 50.0, 0.0); // June 21 noon UTC
        assert!(north_summer.up > north_winter.up+0.4);
        assert!(position(DAY+5.0*3600.0, 10.94469, 106.82432).up > 0.8);
        assert!(position(DAY+5.0*3600.0, 10.94469, -73.0).up < 0.0);
    }

    #[test]
    fn fallback_peaks_at_noon_and_has_no_night_shadow() {
        assert!(clock(0.5).up > 0.99);
        assert_eq!(shadow(clock(0.0)).2, 0.0);
        assert_eq!(shadow(clock(0.9)).2, 0.0);
    }
}
