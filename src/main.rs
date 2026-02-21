///! Prayer Times Calculator — Egyptian General Authority of Survey
///! Usage:
///!   cargo run -- [OPTIONS]
///!   ./prayer_times [OPTIONS]
///!
///! Options:
///!   --lat <f64>        Latitude  (default: 30.0444  — Cairo)
///!   --lon <f64>        Longitude (default: 31.2357  — Cairo)
///!   --tz  <i32>        UTC offset in hours (default: 2 — Egypt Standard Time)
///!   --date <YYYY-MM-DD> Override date (default: system date)
///!
///! Example:
///!   ./prayer_times --lat 30.0444 --lon 31.2357 --tz 2
///!   ./prayer_times --date 2026-06-15

use std::env;
use std::f64::consts::PI;

// ─────────────────────────────────────────────────────────────
// Global defaults — override via CLI flags
// ─────────────────────────────────────────────────────────────
static DEFAULT_LAT: f64 = 30.0444;   // Cairo latitude  (°N)
static DEFAULT_LON: f64 = 31.2357;   // Cairo longitude (°E)
static DEFAULT_TZ:  i32 = 2;         // UTC+2 (Egypt Standard Time)

// Egyptian General Authority of Survey angles
const FAJR_ANGLE:  f64 = 19.5;  // Sun depression angle for Fajr  (°)
const ISHA_ANGLE:  f64 = 17.5;  // Sun depression angle for Isha  (°)
const ASSR_FACTOR: f64 = 1.0;   // 1 = Standard  /  2 = Hanafi

// ─────────────────────────────────────────────────────────────
// Tiny unit helpers
// ─────────────────────────────────────────────────────────────
#[inline] fn d2r(d: f64) -> f64 { d * PI / 180.0 }
#[inline] fn r2d(r: f64) -> f64 { r * 180.0 / PI }

// ─────────────────────────────────────────────────────────────
// Julian Day Number (integer, proleptic Gregorian)
// ─────────────────────────────────────────────────────────────
fn julian_day(year: i32, month: u32, day: u32) -> f64 {
    let (y, m, d) = (year as f64, month as f64, day as f64);
    let a = ((14.0 - m) / 12.0).floor();
    let yy = y + 4800.0 - a;
    let mm = m + 12.0 * a - 3.0;
    d + ((153.0 * mm + 2.0) / 5.0).floor()
        + 365.0 * yy
        + (yy / 4.0).floor()
        - (yy / 100.0).floor()
        + (yy / 400.0).floor()
        - 32045.0
}

// ─────────────────────────────────────────────────────────────
// Solar position → (declination °, equation_of_time hours)
// Based on low-precision USNO / Jean Meeus formulas
// ─────────────────────────────────────────────────────────────
fn sun_position(jd: f64) -> (f64, f64) {
    let d  = jd - 2_451_545.0;                          // days from J2000
    let g  = 357.529 + 0.985_600_28 * d;                // mean anomaly (°)
    let q  = 280.459 + 0.985_647_36 * d;                // mean longitude (°)
    let l  = q + 1.915 * d2r(g).sin() + 0.020 * d2r(2.0 * g).sin(); // ecliptic long (°)
    let e  = 23.439 - 0.000_000_36 * d;                 // obliquity (°)

    // Right ascension (°) via atan2, then normalise to 0..360
    let ra_rad = (d2r(e).cos() * d2r(l).sin()).atan2(d2r(l).cos());
    let mut ra = r2d(ra_rad);
    ra -= 360.0 * (ra / 360.0).floor();

    // Declination (°)
    let dec = r2d((d2r(e).sin() * d2r(l).sin()).asin());

    // Equation of time (hours)  [q and ra both in degrees]
    let eq_t = (q - 0.005_718_3 - ra
        + 0.000_8 * d2r(125.04 - 0.052_954 * d).sin()) / 15.0;

    (dec, eq_t)
}

// ─────────────────────────────────────────────────────────────
// Hour angle for a given target altitude angle (°)
// Returns None if the sun never reaches that altitude
// ─────────────────────────────────────────────────────────────
fn hour_angle(target_alt: f64, dec: f64, lat: f64) -> Option<f64> {
    let cos_ha = (d2r(target_alt).sin()
        - d2r(dec).sin() * d2r(lat).sin())
        / (d2r(dec).cos() * d2r(lat).cos());

    if cos_ha < -1.0 || cos_ha > 1.0 {
        None  // sun never reaches this altitude at this location
    } else {
        Some(r2d(cos_ha.acos()))
    }
}

// ─────────────────────────────────────────────────────────────
// Assr altitude angle from shadow factor + declination/latitude
// ─────────────────────────────────────────────────────────────
fn assr_altitude(lat: f64, dec: f64) -> f64 {
    r2d((1.0 / (ASSR_FACTOR + (lat - dec).abs().to_radians().tan())).atan())
}

// ─────────────────────────────────────────────────────────────
// Format a decimal-hours value as HH:MM:SS
// ─────────────────────────────────────────────────────────────
fn fmt_time(mut t: f64) -> String {
    // Normalise to 0..24
    t -= 24.0 * (t / 24.0).floor();
    let total_secs = (t * 3600.0).round() as u32;
    let h = total_secs / 3600;
    let m = (total_secs % 3600) / 60;
    let s = total_secs % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

// ─────────────────────────────────────────────────────────────
// Core calculation — ONE function, uses global-style config
// ─────────────────────────────────────────────────────────────
fn calc_prayer_times(
    year:  i32,
    month: u32,
    day:   u32,
    lat:   f64,
    lon:   f64,
    tz:    i32,
) {
    let jd = julian_day(year, month, day);
    let (dec, eq_t) = sun_position(jd);

    // Solar noon in UTC (hours)
    let transit = 12.0 - (lon / 15.0) - eq_t;

    // Fajr  — depression FAJR_ANGLE below horizon
    let fajr = hour_angle(-FAJR_ANGLE, dec, lat)
        .map(|ha| transit - ha / 15.0)
        .unwrap_or(f64::NAN);

    // Sunrise / Sunset  — standard refraction + solar disc correction
    let ha_sr   = hour_angle(-0.8333, dec, lat).unwrap_or(f64::NAN);
    let sunrise = transit - ha_sr / 15.0;
    let sunset  = transit + ha_sr / 15.0;

    // Dhuhr — solar noon
    let dhuhr = transit;

    // Assr
    let assr_ang = assr_altitude(lat, dec);
    let assr = hour_angle(assr_ang, dec, lat)
        .map(|ha| transit + ha / 15.0)
        .unwrap_or(f64::NAN);

    // Isha  — depression ISHA_ANGLE below horizon
    let isha = hour_angle(-ISHA_ANGLE, dec, lat)
        .map(|ha| transit + ha / 15.0)
        .unwrap_or(f64::NAN);

    // Shift UTC → local
    let offset = tz as f64;
    let times: Vec<(&str, f64)> = vec![
        ("Fajr   ", fajr    + offset),
        ("Sunrise", sunrise + offset),
        ("Dhuhr  ", dhuhr   + offset),
        ("Assr   ", assr    + offset),
        ("Maghrib", sunset  + offset),
        ("Isha   ", isha    + offset),
    ];

    // ── Output ──────────────────────────────────────────────
    println!("╔══════════════════════════════════════════════╗");
    println!("║        Prayer Times — Cairo, Egypt           ║");
    println!("║  Method: Egyptian General Authority of Survey║");
    println!("╠══════════════════════════════════════════════╣");
    println!("║  Date     : {:04}-{:02}-{:02}                       ║", year, month, day);
    println!("║  Lat/Lon  : {:.4}°N / {:.4}°E            ║", lat, lon);
    println!("║  Timezone : UTC{:+}                            ║", tz);
    println!("╠══════════════════════════════════════════════╣");
    for (name, t) in &times {
        println!("║  {}  {}                           ║", name, fmt_time(*t));
    }
    println!("╚══════════════════════════════════════════════╝");
}

// ─────────────────────────────────────────────────────────────
// System date (no external crates — pure libc time)
// ─────────────────────────────────────────────────────────────
fn system_date() -> (i32, u32, u32) {
    // Obtain UTC epoch seconds via std then convert
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock error")
        .as_secs() as i64;

    // Civil date from Unix timestamp (days since 1970-01-01)
    let days = (secs / 86_400) as i32;
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z  = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe/1460 + doe/36524 - doe/146096) / 365;
    let y   = yoe + era * 400;
    let doy = doe - (365*yoe + yoe/4 - yoe/100);
    let mp  = (5*doy + 2) / 153;
    let d   = doy - (153*mp + 2)/5 + 1;
    let m   = if mp < 10 { mp + 3 } else { mp - 9 };
    let y   = if m <= 2 { y + 1 } else { y };

    (y as i32, m as u32, d as u32)
}

// ─────────────────────────────────────────────────────────────
// CLI argument parser  (no external crates)
// ─────────────────────────────────────────────────────────────
fn parse_args() -> (f64, f64, i32, Option<(i32, u32, u32)>) {
    let args: Vec<String> = env::args().collect();
    let mut lat  = DEFAULT_LAT;
    let mut lon  = DEFAULT_LON;
    let mut tz   = DEFAULT_TZ;
    let mut date = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--lat"  => { i += 1; lat  = args[i].parse().expect("--lat needs a float"); }
            "--lon"  => { i += 1; lon  = args[i].parse().expect("--lon needs a float"); }
            "--tz"   => { i += 1; tz   = args[i].parse().expect("--tz needs an integer"); }
            "--date" => {
                i += 1;
                let parts: Vec<&str> = args[i].split('-').collect();
                if parts.len() != 3 {
                    eprintln!("--date must be YYYY-MM-DD"); std::process::exit(1);
                }
                let y: i32 = parts[0].parse().expect("bad year");
                let m: u32 = parts[1].parse().expect("bad month");
                let d: u32 = parts[2].parse().expect("bad day");
                date = Some((y, m, d));
            }
            "--help" | "-h" => {
                println!("Usage: prayer_times [OPTIONS]");
                println!("  --lat  <float>      Latitude  (default {DEFAULT_LAT})");
                println!("  --lon  <float>      Longitude (default {DEFAULT_LON})");
                println!("  --tz   <int>        UTC offset hours (default {DEFAULT_TZ})");
                println!("  --date <YYYY-MM-DD> Date override (default: today)");
                std::process::exit(0);
            }
            other => { eprintln!("Unknown flag: {other}  (try --help)"); std::process::exit(1); }
        }
        i += 1;
    }
    (lat, lon, tz, date)
}

// ─────────────────────────────────────────────────────────────
// Entry point
// ─────────────────────────────────────────────────────────────
fn main() {
    let (lat, lon, tz, date_override) = parse_args();
    let (year, month, day) = date_override.unwrap_or_else(system_date);
    calc_prayer_times(year, month, day, lat, lon, tz);
}
