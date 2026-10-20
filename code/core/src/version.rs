//! Migration file names: `VYYYY_MM_DD__HH_MM__kebab-case-name.sql`, plus an optional
//! `….undo.sql` sibling.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::Error;

/// A migration version: a timestamp to the minute. Ordering is chronological.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Version {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
}

impl Version {
    /// The current UTC time, to the minute.
    #[must_use]
    pub fn now() -> Version {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        Version::from_unix_seconds(secs)
    }

    /// Convert Unix time to a UTC version (Howard Hinnant's `civil_from_days`).
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_possible_wrap
    )]
    pub fn from_unix_seconds(secs: u64) -> Version {
        let days = (secs / 86_400) as i64;
        let rem = secs % 86_400;
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
        let year = (yoe + era * 400 + i64::from(month <= 2)) as u16;
        Version {
            year,
            month,
            day,
            hour: (rem / 3600) as u8,
            minute: ((rem % 3600) / 60) as u8,
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}_{:02}_{:02}__{:02}_{:02}",
            self.year, self.month, self.day, self.hour, self.minute
        )
    }
}

/// A migration's identity: its version and its name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MigrationId {
    pub version: Version,
    pub name: String,
}

impl MigrationId {
    /// The file stem shared by the up and undo files, e.g. `V2025_01_06__10_15__create-users`.
    #[must_use]
    pub fn stem(&self) -> String {
        format!("V{}__{}", self.version, self.name)
    }

    #[must_use]
    pub fn up_file_name(&self) -> String {
        format!("{}.sql", self.stem())
    }

    #[must_use]
    pub fn undo_file_name(&self) -> String {
        format!("{}.undo.sql", self.stem())
    }
}

impl fmt::Display for MigrationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.stem())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    Up,
    Undo,
}

/// Parse a migration file name.
///
/// Returns `Ok(None)` for files that aren't migrations at all (no leading `V`, or not `.sql`),
/// so a migrations directory can also hold a README, config, etc.
///
/// # Errors
/// A file that looks like a migration (`V….sql`) but doesn't follow the scheme.
pub fn parse_file_name(file_name: &str) -> Result<Option<(MigrationId, FileKind)>, Error> {
    if !file_name.starts_with('V') || !file_name.ends_with(".sql") {
        return Ok(None);
    }
    let invalid = |reason: &str| Error::InvalidFileName {
        name: file_name.to_string(),
        reason: format!("{reason} (expected VYYYY_MM_DD__HH_MM__kebab-case-name.sql or .undo.sql)"),
    };

    let (stem, kind) = if let Some(stem) = file_name.strip_suffix(".undo.sql") {
        (stem, FileKind::Undo)
    } else {
        (
            file_name.strip_suffix(".sql").unwrap_or(file_name),
            FileKind::Up,
        )
    };
    let rest = &stem[1..];

    // "YYYY_MM_DD__HH_MM__name": the timestamp is a fixed 17 characters.
    if rest.len() < 17 + 2 + 1 || !rest.is_char_boundary(17) {
        return Err(invalid("too short"));
    }
    let (timestamp, after) = rest.split_at(17);
    let name = after
        .strip_prefix("__")
        .ok_or_else(|| invalid("missing `__` between the timestamp and the name"))?;

    let version = parse_timestamp(timestamp).ok_or_else(|| invalid("bad timestamp"))?;
    if !is_kebab_case(name) {
        return Err(invalid(
            "name must be kebab-case (a-z, 0-9, single hyphens)",
        ));
    }

    Ok(Some((
        MigrationId {
            version,
            name: name.to_string(),
        },
        kind,
    )))
}

fn parse_timestamp(s: &str) -> Option<Version> {
    // YYYY_MM_DD__HH_MM
    let b = s.as_bytes();
    if b.len() != 17 || b[4] != b'_' || b[7] != b'_' || &s[10..12] != "__" || b[14] != b'_' {
        return None;
    }
    let num = |range: std::ops::Range<usize>| -> Option<u16> {
        let part = &s[range];
        if part.bytes().all(|c| c.is_ascii_digit()) {
            part.parse().ok()
        } else {
            None
        }
    };
    let year = num(0..4)?;
    let month = u8::try_from(num(5..7)?).ok()?;
    let day = u8::try_from(num(8..10)?).ok()?;
    let hour = u8::try_from(num(12..14)?).ok()?;
    let minute = u8::try_from(num(15..17)?).ok()?;

    let valid = (1..=12).contains(&month)
        && day >= 1
        && day <= days_in_month(year, month)
        && hour < 24
        && minute < 60;
    valid.then_some(Version {
        year,
        month,
        day,
        hour,
        minute,
    })
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 if (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400) => {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn is_kebab_case(name: &str) -> bool {
    !name.is_empty()
        && name.split('-').all(|word| {
            !word.is_empty()
                && word
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_up_and_undo() {
        let (id, kind) = parse_file_name("V2025_01_06__10_15__create-users.sql")
            .unwrap()
            .unwrap();
        assert_eq!(kind, FileKind::Up);
        assert_eq!(id.name, "create-users");
        assert_eq!(id.version.to_string(), "2025_01_06__10_15");
        assert_eq!(id.up_file_name(), "V2025_01_06__10_15__create-users.sql");

        let (undo, kind) = parse_file_name("V2025_01_06__10_15__create-users.undo.sql")
            .unwrap()
            .unwrap();
        assert_eq!(kind, FileKind::Undo);
        assert_eq!(undo, id);
    }

    #[test]
    fn ignores_non_migrations() {
        assert!(parse_file_name("README.md").unwrap().is_none());
        assert!(parse_file_name("config.toml").unwrap().is_none());
        assert!(parse_file_name("seed.sql").unwrap().is_none());
    }

    #[test]
    fn rejects_bad_names() {
        for bad in [
            "V2025_13_06__10_15__x.sql",
            "V2025_02_30__10_15__x.sql",
            "V2025_01_06__24_00__x.sql",
            "V2025_01_06__10_15_x.sql",
            "V2025_01_06__10_15__Create_Users.sql",
            "V2025_01_06__10_15__.sql",
            "V1__init.sql",
        ] {
            assert!(parse_file_name(bad).is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn converts_unix_time() {
        assert_eq!(
            Version::from_unix_seconds(0).to_string(),
            "1970_01_01__00_00"
        );
        assert_eq!(
            Version::from_unix_seconds(1_760_000_000).to_string(),
            "2025_10_09__08_53"
        );
        assert_eq!(
            Version::from_unix_seconds(951_782_400).to_string(),
            "2000_02_29__00_00"
        );
    }

    #[test]
    fn orders_chronologically() {
        let a = parse_file_name("V2024_12_31__23_59__a.sql")
            .unwrap()
            .unwrap()
            .0;
        let b = parse_file_name("V2025_01_01__00_00__b.sql")
            .unwrap()
            .unwrap()
            .0;
        assert!(a.version < b.version);
    }
}
