//! A cached fact about something remote (an engine, a peer, a server) answers [`Probed`], never a
//! bare `Option`.
//!
//! An `Option` has one `None` for two different states: "nobody has asked yet" and "asked, and
//! there is none". A caller that reads `None` cannot tell whether to ask or to refuse, and whichever
//! it picks is wrong for the other state. On the M5 2026-11-04 14:56Z the slot directory's pool
//! accessor answered `None` for an adopted engine nobody had probed yet; the replacement's
//! checkpoint read that as "there is no pool" and refused pool growth, so every resident's window
//! stayed ~24k. With `Probed` the caller matches three variants and the compiler makes it decide
//! each one: ask on [`Probed::NotAsked`], refuse with the typed reason on [`Probed::Absent`],
//! proceed on [`Probed::Known`].

/// What a cache of a remote fact knows: not yet asked, asked and absent (with why), or known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Probed<T, Why> {
    /// Nobody has asked the remote side yet: the answer is to ask, never to refuse.
    NotAsked,
    /// Asked, and the remote side has none, for the typed reason `Why`.
    Absent(Why),
    /// Asked, and this is the answer.
    Known(T),
}

impl<T, Why> Probed<T, Why> {
    /// Borrow the fact without giving up the three states.
    pub fn as_ref(&self) -> Probed<&T, &Why> {
        match self {
            Self::NotAsked => Probed::NotAsked,
            Self::Absent(why) => Probed::Absent(why),
            Self::Known(fact) => Probed::Known(fact),
        }
    }
}

impl<T: Clone, Why: Clone> Probed<&T, &Why> {
    /// An owned copy of a borrowed answer, still in its three states.
    pub fn cloned(self) -> Probed<T, Why> {
        match self {
            Probed::NotAsked => Probed::NotAsked,
            Probed::Absent(why) => Probed::Absent(why.clone()),
            Probed::Known(fact) => Probed::Known(fact.clone()),
        }
    }
}
