//! Lever et coucher du soleil, calculés localement (§7).
//!
//! Aucun appel réseau : l'application est hors ligne par construction, et une
//! dépendance externe à l'exécution la rendrait inutilisable sans connexion.
//!
//! Algorithme NOAA, précision de l'ordre de la minute — largement suffisant pour
//! dire si un passage a eu lieu de jour, de nuit ou au crépuscule.

use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};

/// Où se situe un instant par rapport au soleil.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Day,
    /// Crépuscule civil du matin.
    Dawn,
    /// Crépuscule civil du soir.
    Dusk,
    Night,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Day => "day",
            Phase::Dawn => "dawn",
            Phase::Dusk => "dusk",
            Phase::Night => "night",
        }
    }
}

/// Les instants solaires d'un jour, en minutes depuis minuit, heure murale locale.
#[derive(Debug, Clone, Copy)]
pub struct SunTimes {
    pub sunrise: Option<f64>,
    pub sunset: Option<f64>,
    pub dawn: Option<f64>,
    pub dusk: Option<f64>,
}

/// Angle du soleil sous l'horizon définissant chaque instant.
const ZENITH_OFFICIAL: f64 = 90.833; // lever / coucher, réfraction comprise
const ZENITH_CIVIL: f64 = 96.0; // crépuscule civil

/// Calcule les instants solaires.
///
/// `utc_offset_hours` dit de combien l'heure murale de la caméra est décalée par
/// rapport à UTC. Les dates de l'application sont de l'heure murale au piège
/// (voir `format.ts`) : sans ce décalage, on comparerait un midi local à un midi UTC.
pub fn times(date: NaiveDate, lat: f64, lng: f64, utc_offset_hours: f64) -> SunTimes {
    SunTimes {
        sunrise: event(date, lat, lng, utc_offset_hours, ZENITH_OFFICIAL, true),
        sunset: event(date, lat, lng, utc_offset_hours, ZENITH_OFFICIAL, false),
        dawn: event(date, lat, lng, utc_offset_hours, ZENITH_CIVIL, true),
        dusk: event(date, lat, lng, utc_offset_hours, ZENITH_CIVIL, false),
    }
}

/// `None` quand l'événement n'a pas lieu ce jour-là : au-delà des cercles polaires,
/// le soleil peut ne pas se lever du tout. Mieux vaut l'absence qu'une heure inventée.
fn event(
    date: NaiveDate,
    lat: f64,
    lng: f64,
    utc_offset_hours: f64,
    zenith: f64,
    rising: bool,
) -> Option<f64> {
    let day = date.ordinal() as f64;
    let lng_hour = lng / 15.0;

    let t = if rising {
        day + ((6.0 - lng_hour) / 24.0)
    } else {
        day + ((18.0 - lng_hour) / 24.0)
    };

    // Anomalie moyenne du soleil.
    let m = (0.9856 * t) - 3.289;
    // Longitude vraie.
    let mut l = m + (1.916 * m.to_radians().sin()) + (0.020 * (2.0 * m).to_radians().sin()) + 282.634;
    l = wrap(l, 360.0);

    // Ascension droite, ramenée dans le même quadrant que la longitude.
    let mut ra = (0.91764 * l.to_radians().tan()).atan().to_degrees();
    ra = wrap(ra, 360.0);
    let l_quadrant = (l / 90.0).floor() * 90.0;
    let ra_quadrant = (ra / 90.0).floor() * 90.0;
    ra = (ra + (l_quadrant - ra_quadrant)) / 15.0;

    // Déclinaison.
    let sin_dec = 0.39782 * l.to_radians().sin();
    let cos_dec = sin_dec.asin().cos();

    let cos_h = (zenith.to_radians().cos() - (sin_dec * lat.to_radians().sin()))
        / (cos_dec * lat.to_radians().cos());
    if !(-1.0..=1.0).contains(&cos_h) {
        // Soleil toujours levé, ou jamais.
        return None;
    }

    let h = if rising {
        360.0 - cos_h.acos().to_degrees()
    } else {
        cos_h.acos().to_degrees()
    } / 15.0;

    let mean_time = h + ra - (0.06571 * t) - 6.622;
    let ut = wrap(mean_time - lng_hour, 24.0);
    let local = wrap(ut + utc_offset_hours, 24.0);
    Some(local * 60.0)
}

fn wrap(value: f64, modulus: f64) -> f64 {
    let r = value % modulus;
    if r < 0.0 {
        r + modulus
    } else {
        r
    }
}

/// Classe un instant. `None` quand la position du piège est inconnue : on ne devine pas.
pub fn phase(at: NaiveDateTime, lat: f64, lng: f64, utc_offset_hours: f64) -> Phase {
    let t = times(at.date(), lat, lng, utc_offset_hours);
    let minutes = at.hour() as f64 * 60.0 + at.minute() as f64;

    match (t.sunrise, t.sunset, t.dawn, t.dusk) {
        (Some(sunrise), Some(sunset), Some(dawn), Some(dusk)) => {
            if minutes >= sunrise && minutes <= sunset {
                Phase::Day
            } else if minutes >= dawn && minutes < sunrise {
                Phase::Dawn
            } else if minutes > sunset && minutes <= dusk {
                Phase::Dusk
            } else {
                Phase::Night
            }
        }
        // Jour ou nuit polaire : pas de lever, donc pas de crépuscule non plus.
        (None, None, _, _) => {
            // Au-delà du cercle polaire, la hauteur du soleil à midi tranche.
            if lat >= 0.0 {
                if at.date().ordinal() > 80 && at.date().ordinal() < 266 {
                    Phase::Day
                } else {
                    Phase::Night
                }
            } else if at.date().ordinal() > 80 && at.date().ordinal() < 266 {
                Phase::Night
            } else {
                Phase::Day
            }
        }
        _ => Phase::Night,
    }
}

/// Minutes écoulées depuis le coucher du soleil (négatif avant).
///
/// C'est la mesure qui compte pour un animal : un renard sort « au crépuscule »,
/// pas « à 18 h ». En heure civile, la même habitude semble se déplacer de plusieurs
/// heures entre juin et décembre.
pub fn minutes_from_sunset(at: NaiveDateTime, lat: f64, lng: f64, utc_offset_hours: f64) -> Option<f64> {
    let t = times(at.date(), lat, lng, utc_offset_hours);
    let sunset = t.sunset?;
    let minutes = at.hour() as f64 * 60.0 + at.minute() as f64;
    let mut delta = minutes - sunset;
    // Ramené dans [-12 h, +12 h] : 2 h du matin est « 8 h après le coucher »,
    // pas « 16 h avant le coucher du même jour ».
    if delta < -720.0 {
        delta += 1440.0;
    } else if delta > 720.0 {
        delta -= 1440.0;
    }
    Some(delta)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dijon, à peu près : 47,32 N — 5,04 E.
    const LAT: f64 = 47.32;
    const LNG: f64 = 5.04;

    fn hhmm(minutes: f64) -> (u32, u32) {
        let m = minutes.round() as u32;
        (m / 60, m % 60)
    }

    #[test]
    fn solstice_d_ete_jour_long() {
        // 21 juin 2026, heure d'été (UTC+2). Éphémérides : lever ~5h40, coucher ~21h50.
        let t = times(NaiveDate::from_ymd_opt(2026, 6, 21).unwrap(), LAT, LNG, 2.0);
        let (h_rise, _) = hhmm(t.sunrise.unwrap());
        let (h_set, _) = hhmm(t.sunset.unwrap());
        assert_eq!(h_rise, 5, "lever attendu vers 5 h, obtenu {:?}", hhmm(t.sunrise.unwrap()));
        assert_eq!(h_set, 21, "coucher attendu vers 21 h, obtenu {:?}", hhmm(t.sunset.unwrap()));
    }

    #[test]
    fn solstice_d_hiver_jour_court() {
        // 21 décembre 2026, heure d'hiver (UTC+1). Lever ~8h20, coucher ~16h50.
        let t = times(NaiveDate::from_ymd_opt(2026, 12, 21).unwrap(), LAT, LNG, 1.0);
        let (h_rise, _) = hhmm(t.sunrise.unwrap());
        let (h_set, _) = hhmm(t.sunset.unwrap());
        assert_eq!(h_rise, 8, "obtenu {:?}", hhmm(t.sunrise.unwrap()));
        assert_eq!(h_set, 16, "obtenu {:?}", hhmm(t.sunset.unwrap()));
    }

    #[test]
    fn classe_jour_nuit_et_crepuscules() {
        let june = NaiveDate::from_ymd_opt(2026, 6, 21).unwrap();
        let at = |h, m| june.and_hms_opt(h, m, 0).unwrap();

        assert_eq!(phase(at(13, 0), LAT, LNG, 2.0), Phase::Day);
        assert_eq!(phase(at(1, 0), LAT, LNG, 2.0), Phase::Night);
        assert_eq!(phase(at(22, 15), LAT, LNG, 2.0), Phase::Dusk);
        assert_eq!(phase(at(5, 10), LAT, LNG, 2.0), Phase::Dawn);
    }

    #[test]
    fn la_meme_habitude_ne_derive_pas_en_heure_solaire() {
        // Un animal qui sort une heure après le coucher, en juin et en décembre.
        let june = NaiveDate::from_ymd_opt(2026, 6, 21)
            .unwrap()
            .and_hms_opt(22, 50, 0)
            .unwrap();
        let december = NaiveDate::from_ymd_opt(2026, 12, 21)
            .unwrap()
            .and_hms_opt(17, 50, 0)
            .unwrap();

        let a = minutes_from_sunset(june, LAT, LNG, 2.0).unwrap();
        let b = minutes_from_sunset(december, LAT, LNG, 1.0).unwrap();

        // Cinq heures d'écart en heure civile, moins d'une demi-heure en heure solaire.
        assert!(
            (a - b).abs() < 30.0,
            "en heure solaire les deux sorties se ressemblent : {a:.0} et {b:.0} minutes"
        );
    }

    #[test]
    fn deux_heures_du_matin_est_apres_le_coucher_pas_avant() {
        let at = NaiveDate::from_ymd_opt(2026, 12, 21)
            .unwrap()
            .and_hms_opt(2, 0, 0)
            .unwrap();
        let d = minutes_from_sunset(at, LAT, LNG, 1.0).unwrap();
        assert!(d > 0.0, "attendu « après le coucher », obtenu {d:.0} minutes");
        assert!(d < 720.0);
    }

    #[test]
    fn nuit_polaire_ne_panique_pas() {
        // Tromsø en décembre : le soleil ne se lève pas.
        let at = NaiveDate::from_ymd_opt(2026, 12, 21)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        assert_eq!(phase(at, 69.65, 18.96, 1.0), Phase::Night);
    }
}
