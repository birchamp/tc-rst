//! Roving focus: the item of a list that an arrow, Home or End moves to (tC4's `Layer`
//! navigation, `Switcher`, `AccountMenu` and the Check rail's `railKeyDown`). Pure, so the host
//! tests it.

/// A key that moves along a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    Next,
    Prev,
    First,
    Last,
}

/// Which arrows a list answers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Down and Up.
    Vertical,
    /// Right and Left.
    Horizontal,
    /// Either pair (tC4's `Switcher`).
    Both,
}

impl Move {
    /// The move a key makes on a list of `axis`; `None` for a key the list leaves alone.
    pub fn of_key(key: &str, axis: Axis) -> Option<Self> {
        let vertical = axis != Axis::Horizontal;
        let horizontal = axis != Axis::Vertical;
        match key {
            "Home" => Some(Self::First),
            "End" => Some(Self::Last),
            "ArrowDown" if vertical => Some(Self::Next),
            "ArrowUp" if vertical => Some(Self::Prev),
            "ArrowRight" if horizontal => Some(Self::Next),
            "ArrowLeft" if horizontal => Some(Self::Prev),
            _ => None,
        }
    }
}

/// The item a move goes to in a list of `len` items, wrapping at both ends. `current` is the
/// item that has focus; with none, Next goes to the first item and Prev to the last. `None`
/// for an empty list.
pub fn roving(len: usize, current: Option<usize>, step: Move) -> Option<usize> {
    let last = len.checked_sub(1)?;
    Some(match (step, current) {
        (Move::First, _) | (Move::Next, None) => 0,
        (Move::Last, _) | (Move::Prev, None) => last,
        (Move::Next, Some(at)) if at < last => at + 1,
        (Move::Next, Some(_)) => 0,
        (Move::Prev, Some(at)) => at.checked_sub(1).unwrap_or(last),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_step_and_wrap_home_and_end_jump() {
        assert_eq!(roving(3, Some(0), Move::Next), Some(1));
        assert_eq!(roving(3, Some(2), Move::Next), Some(0));
        assert_eq!(roving(3, Some(1), Move::Prev), Some(0));
        assert_eq!(roving(3, Some(0), Move::Prev), Some(2));
        assert_eq!(roving(3, Some(1), Move::First), Some(0));
        assert_eq!(roving(3, Some(1), Move::Last), Some(2));
    }

    #[test]
    fn with_nothing_focused_next_goes_first_and_prev_goes_last() {
        assert_eq!(roving(3, None, Move::Next), Some(0));
        assert_eq!(roving(3, None, Move::Prev), Some(2));
        assert_eq!(roving(1, None, Move::Prev), Some(0));
    }

    #[test]
    fn an_empty_list_moves_nowhere() {
        for step in [Move::Next, Move::Prev, Move::First, Move::Last] {
            assert_eq!(roving(0, None, step), None);
        }
    }

    #[test]
    fn each_axis_answers_to_its_own_arrows() {
        assert_eq!(Move::of_key("ArrowDown", Axis::Vertical), Some(Move::Next));
        assert_eq!(Move::of_key("ArrowRight", Axis::Vertical), None);
        assert_eq!(
            Move::of_key("ArrowLeft", Axis::Horizontal),
            Some(Move::Prev)
        );
        assert_eq!(Move::of_key("ArrowUp", Axis::Horizontal), None);
        assert_eq!(Move::of_key("ArrowUp", Axis::Both), Some(Move::Prev));
        assert_eq!(Move::of_key("ArrowRight", Axis::Both), Some(Move::Next));
        assert_eq!(Move::of_key("End", Axis::Horizontal), Some(Move::Last));
        assert_eq!(Move::of_key("Tab", Axis::Both), None);
    }
}
