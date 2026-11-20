//! `Uncertain<T>`: first-class missing data (design.txt §4.A.3).
//!
//! An unknown attribute is resolved to the treatment the bank can evidence
//! without it (the "conservative" value) for the reported figure, and to the
//! favourable value for an "optimistic" run; the difference between the two
//! runs is the cost of the missing data. Every resolution is logged.
//!
//! Note: the evidence-based value is usually, but not always, the one with the
//! higher run-off. Where a favourable treatment needs evidence (e.g. SME retail
//! status), the default is the treatment that applies without it, even if that
//! happens to be cheaper on one metric. See `deposits` for the SME case.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Uncertain<T> {
    Known(T),
    Unknown(String),
}

impl<T> Uncertain<T> {
    pub fn unknown(reason: impl Into<String>) -> Self {
        Uncertain::Unknown(reason.into())
    }

    pub fn known(&self) -> Option<&T> {
        match self {
            Uncertain::Known(v) => Some(v),
            Uncertain::Unknown(_) => None,
        }
    }
}

impl<T> From<T> for Uncertain<T> {
    fn from(v: T) -> Self {
        Uncertain::Known(v)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Unknowns take the evidence-based (prudent) value. What gets reported.
    Conservative,
    /// Unknowns take the favourable value. Used only to price missing data.
    Optimistic,
}

/// A logged resolution of an unknown attribute.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assumption {
    pub subject: String,
    pub attribute: &'static str,
    pub assumed: String,
    pub reason: String,
}

impl fmt::Display for Assumption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{} assumed {} ({})", self.subject, self.attribute, self.assumed, self.reason)
    }
}

/// Collects assumptions made while resolving unknowns under one mode.
#[derive(Debug)]
pub struct Resolver {
    pub mode: Mode,
    pub log: Vec<Assumption>,
}

impl Resolver {
    pub fn new(mode: Mode) -> Self {
        Resolver { mode, log: Vec::new() }
    }

    /// Resolve `u`. `worst` is the prudentially conservative value, `best` the
    /// favourable one.
    pub fn resolve<T: Clone + fmt::Debug>(
        &mut self,
        u: &Uncertain<T>,
        worst: T,
        best: T,
        subject: &str,
        attribute: &'static str,
    ) -> T {
        match u {
            Uncertain::Known(v) => v.clone(),
            Uncertain::Unknown(reason) => {
                let v = match self.mode {
                    Mode::Conservative => worst,
                    Mode::Optimistic => best,
                };
                self.log.push(Assumption {
                    subject: subject.to_string(),
                    attribute,
                    assumed: format!("{v:?}"),
                    reason: reason.clone(),
                });
                v
            }
        }
    }

    /// Booleans where `true` is the conservative value (e.g. "is online").
    pub fn worst_true(&mut self, u: &Uncertain<bool>, subject: &str, attribute: &'static str) -> bool {
        self.resolve(u, true, false, subject, attribute)
    }

    /// Booleans where `false` is the conservative value (e.g. "has relationship").
    pub fn worst_false(&mut self, u: &Uncertain<bool>, subject: &str, attribute: &'static str) -> bool {
        self.resolve(u, false, true, subject, attribute)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknowns_resolve_by_mode_and_are_logged() {
        let u: Uncertain<bool> = Uncertain::unknown("no CRM link");
        let mut c = Resolver::new(Mode::Conservative);
        assert!(!c.worst_false(&u, "cust-1", "established_relationship"));
        assert_eq!(c.log.len(), 1);
        let mut o = Resolver::new(Mode::Optimistic);
        assert!(o.worst_false(&u, "cust-1", "established_relationship"));
        let k: Uncertain<bool> = true.into();
        assert!(c.worst_false(&k, "cust-1", "established_relationship"));
        assert_eq!(c.log.len(), 1, "known values are not assumptions");
    }
}
