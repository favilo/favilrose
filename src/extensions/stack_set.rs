use std::{
    cmp::{Ordering, Reverse},
    hash::Hash,
};

use anyhow::{anyhow, Result};
use derive_more::Debug;
use penrose::pure::{geometry::Rect, Position, Stack, StackSet};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Left,
    Down,
    Up,
    Right,
}

pub trait StackSetExt<C>
where
    C: Clone + Eq + Hash + Ord,
{
    fn neighbor_in_direction<F>(
        &self,
        direction: Direction,
        preferred: Option<&C>,
        geometry: F,
    ) -> Option<C>
    where
        F: FnMut(&C) -> Option<Rect>;

    fn focus_in_direction<F>(
        &mut self,
        direction: Direction,
        preferred: Option<&C>,
        geometry: F,
    ) -> Option<C>
    where
        F: FnMut(&C) -> Option<Rect>;

    /// Swap the focused client with the selected directional neighbor.
    ///
    /// # Errors
    ///
    /// Returns an error if the current stack cannot be mutated without losing clients.
    fn swap_in_direction<F>(
        &mut self,
        direction: Direction,
        preferred: Option<&C>,
        geometry: F,
    ) -> Result<Option<C>>
    where
        F: FnMut(&C) -> Option<Rect>;
}

impl<C> StackSetExt<C> for StackSet<C>
where
    C: Clone + Eq + Hash + Ord,
{
    fn neighbor_in_direction<F>(
        &self,
        direction: Direction,
        preferred: Option<&C>,
        mut geometry: F,
    ) -> Option<C>
    where
        F: FnMut(&C) -> Option<Rect>,
    {
        let focused = self.current_client()?.clone();
        let focused_rect = geometry(&focused)?;

        self.current_workspace()
            .clients()
            .filter(|client| *client != &focused)
            .filter_map(|client| {
                let rank = candidate_rank(
                    focused_rect,
                    geometry(client)?,
                    direction,
                    preferred.is_some_and(|preferred| preferred == client),
                )?;
                Some((rank, client))
            })
            .min_by(|(left_rank, left_client), (right_rank, right_client)| {
                left_rank
                    .cmp(right_rank)
                    .then_with(|| left_client.cmp(right_client))
            })
            .map(|(_, client)| client.clone())
    }

    fn focus_in_direction<F>(
        &mut self,
        direction: Direction,
        preferred: Option<&C>,
        geometry: F,
    ) -> Option<C>
    where
        F: FnMut(&C) -> Option<Rect>,
    {
        let neighbor = self.neighbor_in_direction(direction, preferred, geometry)?;
        self.focus_client(&neighbor);

        Some(neighbor)
    }

    fn swap_in_direction<F>(
        &mut self,
        direction: Direction,
        preferred: Option<&C>,
        geometry: F,
    ) -> Result<Option<C>>
    where
        F: FnMut(&C) -> Option<Rect>,
    {
        let Some(neighbor) = self.neighbor_in_direction(direction, preferred, geometry) else {
            return Ok(None);
        };
        let focused = self
            .current_client()
            .cloned()
            .ok_or_else(|| anyhow!("directional swap has no focused client"))?;
        let mut outcome = Err(anyhow!("directional swap has no occupied stack"));

        self.modify_occupied(|stack| {
            let original = stack.clone();

            match swap_clients(stack, &focused, &neighbor) {
                Ok(swapped) => {
                    outcome = Ok(());
                    swapped
                }
                Err(error) => {
                    outcome = Err(error);
                    original
                }
            }
        });
        outcome?;

        Ok(Some(neighbor))
    }
}

fn candidate_rank(
    focused: Rect,
    candidate: Rect,
    direction: Direction,
    is_preferred: bool,
) -> Option<CandidateRank> {
    let focused_left = i64::from(focused.x);
    let focused_top = i64::from(focused.y);
    let focused_right = focused_left + i64::from(focused.w);
    let focused_bottom = focused_top + i64::from(focused.h);
    let candidate_left = i64::from(candidate.x);
    let candidate_top = i64::from(candidate.y);
    let candidate_right = candidate_left + i64::from(candidate.w);
    let candidate_bottom = candidate_top + i64::from(candidate.h);

    let (overlap, distance, midpoint_offset) = match direction {
        Direction::Left if candidate_right <= focused_left => (
            overlap(candidate_top, candidate_bottom, focused_top, focused_bottom),
            focused_left - candidate_right,
            midpoint_offset(candidate_top, candidate_bottom, focused_top, focused_bottom),
        ),
        Direction::Right if candidate_left >= focused_right => (
            overlap(candidate_top, candidate_bottom, focused_top, focused_bottom),
            candidate_left - focused_right,
            midpoint_offset(candidate_top, candidate_bottom, focused_top, focused_bottom),
        ),
        Direction::Up if candidate_bottom <= focused_top => (
            overlap(candidate_left, candidate_right, focused_left, focused_right),
            focused_top - candidate_bottom,
            midpoint_offset(candidate_left, candidate_right, focused_left, focused_right),
        ),
        Direction::Down if candidate_top >= focused_bottom => (
            overlap(candidate_left, candidate_right, focused_left, focused_right),
            candidate_top - focused_bottom,
            midpoint_offset(candidate_left, candidate_right, focused_left, focused_right),
        ),
        _ => return None,
    };

    Some(CandidateRank {
        edge_distance: distance,
        is_not_preferred: !is_preferred,
        perpendicular_overlap: Reverse(overlap),
        perpendicular_midpoint_offset: midpoint_offset,
    })
}

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
struct CandidateRank {
    edge_distance: i64,
    is_not_preferred: bool,
    perpendicular_overlap: Reverse<i64>,
    perpendicular_midpoint_offset: i64,
}

fn overlap(first_start: i64, first_end: i64, second_start: i64, second_end: i64) -> i64 {
    first_end
        .min(second_end)
        .saturating_sub(first_start.max(second_start))
}

fn midpoint_offset(first_start: i64, first_end: i64, second_start: i64, second_end: i64) -> i64 {
    let first_midpoint = first_start + (first_end - first_start) / 2;
    let second_midpoint = second_start + (second_end - second_start) / 2;

    (first_midpoint - second_midpoint).abs()
}

fn swap_clients<C>(stack: Stack<C>, focused: &C, neighbor: &C) -> Result<Stack<C>>
where
    C: Clone + Eq,
{
    let focused_index = stack
        .iter()
        .position(|client| client == focused)
        .ok_or_else(|| anyhow!("focused client is not in the current stack"))?;
    let neighbor_index = stack
        .iter()
        .position(|client| client == neighbor)
        .ok_or_else(|| anyhow!("selected neighbor is not in the current stack"))?;
    let (focused, remainder) = stack.remove_focused();
    let mut remainder = remainder.ok_or_else(|| anyhow!("directional swap requires a neighbor"))?;

    if remainder.len() == 1 {
        let position = if focused_index < neighbor_index {
            Position::After
        } else {
            Position::Before
        };
        remainder.insert_at(position, focused.clone());
        remainder.focus_element(&focused);
        return Ok(remainder);
    }

    let (neighbor, remainder) = remainder.remove(neighbor);
    let neighbor = neighbor.ok_or_else(|| anyhow!("selected neighbor could not be removed"))?;
    let mut remainder = remainder.ok_or_else(|| anyhow!("directional swap leaves no clients"))?;

    match focused_index.cmp(&neighbor_index) {
        Ordering::Less => {
            insert_at_index(&mut remainder, focused_index, neighbor)?;
            insert_at_index(&mut remainder, neighbor_index, focused.clone())?;
        }
        Ordering::Greater => {
            insert_at_index(&mut remainder, neighbor_index, focused.clone())?;
            insert_at_index(&mut remainder, focused_index, neighbor)?;
        }
        Ordering::Equal => return Err(anyhow!("directional neighbor is the focused client")),
    }

    remainder.focus_element(&focused);
    Ok(remainder)
}

fn insert_at_index<C>(stack: &mut Stack<C>, index: usize, client: C) -> Result<()>
where
    C: Clone + Eq,
{
    if index == stack.len() {
        stack.insert_at(Position::Tail, client);
        return Ok(());
    }

    let anchor = stack
        .iter()
        .nth(index)
        .cloned()
        .ok_or_else(|| anyhow!("insertion index is outside the stack"))?;
    stack.focus_element(&anchor);
    stack.insert_at(Position::Before, client);

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use penrose::{core::layout::LayoutStack, pure::StackSet};

    use super::*;

    fn stack_set(clients: &[u8], focused: u8) -> StackSet<u8> {
        let mut stack_set = StackSet::try_new(
            LayoutStack::default(),
            ["one"],
            [Rect::new(0, 0, 1_000, 1_000)],
        )
        .expect("create stack set");

        for client in clients {
            stack_set.insert_at(Position::Tail, *client);
        }
        stack_set.focus_client(&focused);

        stack_set
    }

    fn clients(stack_set: &StackSet<u8>) -> Vec<u8> {
        stack_set
            .current_stack()
            .expect("occupied stack")
            .iter()
            .copied()
            .collect()
    }

    #[test]
    fn empty_workspace_has_no_directional_neighbor() {
        let stack_set = StackSet::<u8>::try_new(
            LayoutStack::default(),
            ["one"],
            [Rect::new(0, 0, 1_000, 1_000)],
        )
        .expect("create stack set");

        assert_eq!(
            stack_set.neighbor_in_direction(Direction::Right, None, |_| {
                panic!("geometry should not be queried without a focused client")
            }),
            None
        );
    }

    #[test]
    fn missing_focused_geometry_has_no_directional_neighbor() {
        let stack_set = stack_set(&[1, 2], 1);
        let geometry = HashMap::from([(2, Rect::new(100, 0, 100, 100))]);

        assert_eq!(
            stack_set.neighbor_in_direction(Direction::Right, None, |client| {
                geometry.get(client).copied()
            }),
            None
        );
    }

    #[test]
    fn missing_candidate_geometry_does_not_hide_other_candidates() {
        let stack_set = stack_set(&[1, 2, 3], 1);
        let geometry = HashMap::from([
            (1, Rect::new(0, 0, 100, 100)),
            (3, Rect::new(100, 0, 100, 100)),
        ]);

        assert_eq!(
            stack_set.neighbor_in_direction(Direction::Right, None, |client| {
                geometry.get(client).copied()
            }),
            Some(3)
        );
    }

    #[test]
    fn selects_a_neighbor_in_each_direction() {
        let stack_set = stack_set(&[1, 2, 3, 4, 5], 1);
        let geometry = HashMap::from([
            (1, Rect::new(100, 100, 100, 100)),
            (2, Rect::new(0, 100, 100, 100)),
            (3, Rect::new(100, 200, 100, 100)),
            (4, Rect::new(100, 0, 100, 100)),
            (5, Rect::new(200, 100, 100, 100)),
        ]);

        for (direction, expected) in [
            (Direction::Left, 2),
            (Direction::Down, 3),
            (Direction::Up, 4),
            (Direction::Right, 5),
        ] {
            assert_eq!(
                stack_set.neighbor_in_direction(direction, None, |client| {
                    geometry.get(client).copied()
                }),
                Some(expected)
            );
        }
    }

    #[test]
    fn overlapping_candidate_is_not_directional() {
        let stack_set = stack_set(&[1, 2], 1);
        let geometry = HashMap::from([
            (1, Rect::new(0, 0, 100, 100)),
            (2, Rect::new(50, 0, 100, 100)),
        ]);

        assert_eq!(
            stack_set.neighbor_in_direction(Direction::Right, None, |client| {
                geometry.get(client).copied()
            }),
            None
        );
    }

    #[test]
    fn edge_distance_precedes_perpendicular_overlap() {
        let stack_set = stack_set(&[1, 2, 3], 1);
        let geometry = HashMap::from([
            (1, Rect::new(100, 0, 100, 100)),
            (2, Rect::new(0, 200, 100, 100)),
            (3, Rect::new(-100, 0, 100, 100)),
        ]);

        assert_eq!(
            stack_set.neighbor_in_direction(Direction::Left, None, |client| {
                geometry.get(client).copied()
            }),
            Some(2)
        );
    }

    #[test]
    fn midpoint_alignment_breaks_equal_overlap_ties() {
        let stack_set = stack_set(&[1, 2, 3], 1);
        let geometry = HashMap::from([
            (1, Rect::new(0, 0, 100, 100)),
            (2, Rect::new(100, -50, 100, 100)),
            (3, Rect::new(100, 50, 100, 50)),
        ]);

        assert_eq!(
            stack_set.neighbor_in_direction(Direction::Right, None, |client| {
                geometry.get(client).copied()
            }),
            Some(3)
        );
    }

    #[test]
    fn client_id_breaks_equal_geometry_ties() {
        let stack_set = stack_set(&[1, 3, 2], 1);
        let geometry = HashMap::from([
            (1, Rect::new(0, 0, 100, 100)),
            (2, Rect::new(100, 0, 100, 100)),
            (3, Rect::new(100, 0, 100, 100)),
        ]);

        assert_eq!(
            stack_set.neighbor_in_direction(Direction::Right, None, |client| {
                geometry.get(client).copied()
            }),
            Some(2)
        );
    }

    #[test]
    fn swap_reports_an_invalid_focused_client() {
        let stack = Stack::try_from_iter([1, 2]).expect("create stack");

        let error = swap_clients(stack, &3, &2).expect_err("reject missing focused client");

        assert!(error.to_string().contains("focused client"));
    }

    #[test]
    fn swap_reports_an_invalid_neighbor() {
        let stack = Stack::try_from_iter([1, 2]).expect("create stack");

        let error = swap_clients(stack, &1, &3).expect_err("reject missing neighbor");

        assert!(error.to_string().contains("neighbor"));
    }

    #[test]
    fn chooses_the_candidate_with_the_greatest_perpendicular_overlap() {
        let stack_set = stack_set(&[1, 2, 3], 1);
        let geometry = HashMap::from([
            (1, Rect::new(0, 0, 100, 100)),
            (2, Rect::new(100, 50, 100, 100)),
            (3, Rect::new(100, 0, 100, 100)),
        ]);

        assert_eq!(
            stack_set.neighbor_in_direction(Direction::Right, None, |client| {
                geometry.get(client).copied()
            }),
            Some(3)
        );
    }

    #[test]
    fn tied_edge_distance_prefers_the_remembered_client() {
        let stack_set = stack_set(&[1, 2, 3], 1);
        let geometry = HashMap::from([
            (1, Rect::new(100, 0, 100, 100)),
            (2, Rect::new(0, 0, 100, 100)),
            (3, Rect::new(0, 50, 100, 100)),
        ]);

        assert_eq!(
            stack_set.neighbor_in_direction(Direction::Left, Some(&3), |client| {
                geometry.get(client).copied()
            }),
            Some(3)
        );
    }

    #[test]
    fn preferred_client_does_not_override_a_shorter_edge_distance() {
        let stack_set = stack_set(&[1, 2, 3], 1);
        let geometry = HashMap::from([
            (1, Rect::new(100, 0, 100, 100)),
            (2, Rect::new(0, 0, 100, 100)),
            (3, Rect::new(-100, 0, 100, 100)),
        ]);

        assert_eq!(
            stack_set.neighbor_in_direction(Direction::Left, Some(&3), |client| {
                geometry.get(client).copied()
            }),
            Some(2)
        );
    }

    #[test]
    fn ignores_clients_on_other_screens() {
        let mut stack_set = StackSet::try_new(
            LayoutStack::default(),
            ["one", "two"],
            [
                Rect::new(0, 0, 1_000, 1_000),
                Rect::new(1_000, 0, 1_000, 1_000),
            ],
        )
        .expect("create stack set");
        stack_set.insert_at(Position::Tail, 1);
        stack_set.insert_at(Position::Tail, 2);
        stack_set.focus_screen(1);
        stack_set.insert_at(Position::Tail, 3);
        stack_set.focus_screen(0);
        stack_set.focus_client(&1);
        let geometry = HashMap::from([
            (1, Rect::new(0, 0, 100, 100)),
            (2, Rect::new(300, 0, 100, 100)),
            (3, Rect::new(100, 0, 100, 100)),
        ]);

        assert_eq!(
            stack_set.neighbor_in_direction(Direction::Right, Some(&3), |client| {
                geometry.get(client).copied()
            }),
            Some(2)
        );
    }

    #[test]
    fn preferred_client_outside_the_requested_direction_is_ignored() {
        let stack_set = stack_set(&[1, 2, 3], 1);
        let geometry = HashMap::from([
            (1, Rect::new(100, 0, 100, 100)),
            (2, Rect::new(0, 0, 100, 100)),
            (3, Rect::new(200, 0, 100, 100)),
        ]);

        assert_eq!(
            stack_set.neighbor_in_direction(Direction::Left, Some(&3), |client| {
                geometry.get(client).copied()
            }),
            Some(2)
        );
    }

    #[test]
    fn swaps_non_adjacent_clients_and_preserves_focus() {
        let mut stack_set = stack_set(&[1, 2, 3, 4], 2);
        let geometry = HashMap::from([
            (1, Rect::new(-100, 0, 100, 100)),
            (2, Rect::new(0, 0, 100, 100)),
            (3, Rect::new(-200, 0, 100, 100)),
            (4, Rect::new(100, 0, 100, 100)),
        ]);

        assert_eq!(
            stack_set
                .swap_in_direction(Direction::Right, None, |client| {
                    geometry.get(client).copied()
                })
                .expect("swap clients"),
            Some(4)
        );
        assert_eq!(stack_set.current_client(), Some(&2));
        assert_eq!(
            stack_set
                .current_stack()
                .expect("occupied stack")
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![1, 4, 3, 2]
        );
    }

    #[test]
    fn swaps_adjacent_clients_and_preserves_focus() {
        let mut stack_set = stack_set(&[1, 2, 3], 2);
        let geometry = HashMap::from([
            (1, Rect::new(0, 0, 100, 100)),
            (2, Rect::new(100, 0, 100, 100)),
            (3, Rect::new(200, 0, 100, 100)),
        ]);

        assert_eq!(
            stack_set
                .swap_in_direction(Direction::Right, None, |client| {
                    geometry.get(client).copied()
                })
                .expect("swap clients"),
            Some(3)
        );
        assert_eq!(stack_set.current_client(), Some(&2));
        assert_eq!(clients(&stack_set), vec![1, 3, 2]);
    }

    #[test]
    fn swaps_head_and_tail_clients() {
        let mut stack_set = stack_set(&[1, 2, 3, 4], 4);
        let geometry = HashMap::from([
            (1, Rect::new(0, 0, 100, 100)),
            (4, Rect::new(300, 0, 100, 100)),
        ]);

        assert_eq!(
            stack_set
                .swap_in_direction(Direction::Left, None, |client| {
                    geometry.get(client).copied()
                })
                .expect("swap clients"),
            Some(1)
        );
        assert_eq!(stack_set.current_client(), Some(&4));
        assert_eq!(clients(&stack_set), vec![4, 2, 3, 1]);
    }

    #[test]
    fn swaps_with_a_client_before_focus_and_preserves_intervening_positions() {
        let mut stack_set = stack_set(&[1, 2, 3, 4], 3);
        let geometry = HashMap::from([
            (1, Rect::new(0, 0, 100, 100)),
            (2, Rect::new(200, 0, 100, 100)),
            (3, Rect::new(100, 0, 100, 100)),
            (4, Rect::new(300, 0, 100, 100)),
        ]);

        assert_eq!(
            stack_set
                .swap_in_direction(Direction::Left, None, |client| {
                    geometry.get(client).copied()
                })
                .expect("swap clients"),
            Some(1)
        );
        assert_eq!(stack_set.current_client(), Some(&3));
        assert_eq!(
            stack_set
                .current_stack()
                .expect("occupied stack")
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![3, 2, 1, 4]
        );
    }

    #[test]
    fn focuses_the_selected_directional_client() {
        let mut stack_set = stack_set(&[1, 2], 1);
        let geometry = HashMap::from([
            (1, Rect::new(0, 0, 100, 100)),
            (2, Rect::new(100, 0, 100, 100)),
        ]);

        assert_eq!(
            stack_set.focus_in_direction(Direction::Right, None, |client| {
                geometry.get(client).copied()
            }),
            Some(2)
        );
        assert_eq!(stack_set.current_client(), Some(&2));
    }

    #[test]
    fn swaps_a_two_client_stack_and_preserves_focus() {
        let mut stack_set = stack_set(&[1, 2], 1);
        let geometry = HashMap::from([
            (1, Rect::new(0, 0, 100, 100)),
            (2, Rect::new(100, 0, 100, 100)),
        ]);

        assert_eq!(
            stack_set
                .swap_in_direction(Direction::Right, None, |client| {
                    geometry.get(client).copied()
                })
                .expect("swap clients"),
            Some(2)
        );
        assert_eq!(stack_set.current_client(), Some(&1));
        assert_eq!(
            stack_set
                .current_stack()
                .expect("occupied stack")
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![2, 1]
        );
    }

    #[test]
    fn swaps_a_two_client_stack_when_focus_is_second() {
        let mut stack_set = stack_set(&[1, 2], 2);
        let geometry = HashMap::from([
            (1, Rect::new(0, 0, 100, 100)),
            (2, Rect::new(100, 0, 100, 100)),
        ]);

        assert_eq!(
            stack_set
                .swap_in_direction(Direction::Left, None, |client| {
                    geometry.get(client).copied()
                })
                .expect("swap clients"),
            Some(1)
        );
        assert_eq!(stack_set.current_client(), Some(&2));
        assert_eq!(
            stack_set
                .current_stack()
                .expect("occupied stack")
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![2, 1]
        );
    }
}
