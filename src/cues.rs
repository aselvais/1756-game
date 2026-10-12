//! Sound events. The simulation records these. It does not play audio.
//!
//! `main` drains the queue once per frame. One tick can raise the same event
//! many times (several battles, several floods), so duplicates collapse to a
//! single play of that sound.

/// Something the player should hear.
///
/// The discriminant is the index into the loaded sound bank.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Cue {
    War = 0,
    Alliance,
    Break,
    Peace,
    Annex,
    Fanfare,
    Election,
    Disaster,
    Flood,
    Pirates,
    Founded,
    Built,
    Puppet,
    Freedom,
    Eliminated,
    Surrender,
    Occupy,
    Battle,
    Siege,
    Shot,
    Select,
    Click,
}

impl Cue {
    pub const ALL: [Cue; 22] = [
        Cue::War,
        Cue::Alliance,
        Cue::Break,
        Cue::Peace,
        Cue::Annex,
        Cue::Fanfare,
        Cue::Election,
        Cue::Disaster,
        Cue::Flood,
        Cue::Pirates,
        Cue::Founded,
        Cue::Built,
        Cue::Puppet,
        Cue::Freedom,
        Cue::Eliminated,
        Cue::Surrender,
        Cue::Occupy,
        Cue::Battle,
        Cue::Siege,
        Cue::Shot,
        Cue::Select,
        Cue::Click,
    ];

    pub fn index(self) -> usize {
        self as usize
    }
}

/// First occurrence of each cue, in the order they happened.
pub fn coalesce(cues: impl IntoIterator<Item = Cue>) -> Vec<Cue> {
    let mut seen = [false; Cue::ALL.len()];
    let mut out = Vec::new();
    for cue in cues {
        let i = cue.index();
        if i < seen.len() && !seen[i] {
            seen[i] = true;
            out.push(cue);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalesce_keeps_first_of_each_kind() {
        let cues = coalesce([
            Cue::Peace,
            Cue::War,
            Cue::Peace,
            Cue::Battle,
            Cue::War,
            Cue::Battle,
        ]);
        assert_eq!(cues, vec![Cue::Peace, Cue::War, Cue::Battle]);
    }
}
