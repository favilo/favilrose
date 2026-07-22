use std::collections::{HashMap, HashSet};

use penrose::{
    core::{hooks::StateHook, State, Xid},
    pure::StackSet,
    x::XConn,
};

#[derive(Default)]
pub struct DirectionalNavigationHistory {
    origins_by_destination: HashMap<Xid, Xid>,
    active_workspace_tag: Option<String>,
}

impl DirectionalNavigationHistory {
    pub fn preferred_origin(
        &mut self,
        client_set: &StackSet<Xid>,
        destination: Xid,
    ) -> Option<Xid> {
        self.synchronize(client_set);
        self.origins_by_destination.get(&destination).copied()
    }

    pub fn record(&mut self, client_set: &StackSet<Xid>, origin: Xid, destination: Xid) {
        self.synchronize(client_set);

        let current_workspace = client_set.current_workspace();
        let origin_is_current = current_workspace.clients().any(|client| *client == origin);
        let destination_is_current = current_workspace
            .clients()
            .any(|client| *client == destination);

        if origin_is_current && destination_is_current {
            self.origins_by_destination.insert(destination, origin);
        }
    }

    pub fn synchronize(&mut self, client_set: &StackSet<Xid>) {
        let active_workspace_tag = client_set.current_tag();

        if self.active_workspace_tag.as_deref() != Some(active_workspace_tag) {
            self.origins_by_destination.clear();
            self.active_workspace_tag = Some(active_workspace_tag.to_owned());
        }

        let current_clients = client_set
            .current_workspace()
            .clients()
            .copied()
            .collect::<HashSet<_>>();
        self.origins_by_destination.retain(|destination, origin| {
            current_clients.contains(destination) && current_clients.contains(origin)
        });
    }
}

pub struct DirectionalNavigationHistoryHook;

impl<X> StateHook<X> for DirectionalNavigationHistoryHook
where
    X: XConn,
{
    fn call(&mut self, state: &mut State<X>, _x: &X) -> penrose::Result<()> {
        let history = state.extension_or_default::<DirectionalNavigationHistory>();
        history.borrow_mut().synchronize(&state.client_set);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use penrose::{
        core::layout::LayoutStack,
        pure::{geometry::Rect, Position, StackSet},
    };

    use crate::extensions::stack_set::{Direction, StackSetExt};

    use super::*;

    fn xid(id: u32) -> Xid {
        id.into()
    }

    fn stack_set() -> StackSet<Xid> {
        let mut client_set = StackSet::try_new(
            LayoutStack::default(),
            ["one", "two"],
            [Rect::new(0, 0, 1_000, 1_000)],
        )
        .expect("create stack set");

        for client in [xid(1), xid(2), xid(3)] {
            client_set.insert_at(Position::Tail, client);
        }

        client_set
    }

    #[test]
    fn records_an_origin_for_the_directional_destination() {
        let client_set = stack_set();
        let mut history = DirectionalNavigationHistory::default();

        history.record(&client_set, xid(1), xid(2));

        assert_eq!(history.preferred_origin(&client_set, xid(2)), Some(xid(1)));
    }

    #[test]
    fn latest_origin_replaces_the_previous_origin() {
        let client_set = stack_set();
        let mut history = DirectionalNavigationHistory::default();
        history.record(&client_set, xid(1), xid(2));

        history.record(&client_set, xid(3), xid(2));

        assert_eq!(history.preferred_origin(&client_set, xid(2)), Some(xid(3)));
    }

    #[test]
    fn record_ignores_clients_outside_the_current_workspace() {
        let client_set = stack_set();
        let mut history = DirectionalNavigationHistory::default();

        history.record(&client_set, xid(99), xid(2));
        history.record(&client_set, xid(1), xid(99));

        assert!(history.origins_by_destination.is_empty());
    }

    #[test]
    fn reverse_navigation_prefers_the_last_directional_origin() {
        let mut client_set = stack_set();
        client_set.focus_client(&xid(1));
        let geometry = HashMap::from([
            (xid(1), Rect::new(100, 50, 100, 100)),
            (xid(2), Rect::new(0, 0, 100, 100)),
            (xid(3), Rect::new(100, 0, 100, 100)),
        ]);
        let mut history = DirectionalNavigationHistory::default();
        let origin = *client_set.current_client().expect("focused client");
        let destination = client_set
            .focus_in_direction(Direction::Left, None, |client| {
                geometry.get(client).copied()
            })
            .expect("left neighbor");
        history.record(&client_set, origin, destination);

        let preferred = history.preferred_origin(&client_set, destination);
        let returned =
            client_set.focus_in_direction(Direction::Right, preferred.as_ref(), |client| {
                geometry.get(client).copied()
            });

        assert_eq!(returned, Some(origin));
        assert_eq!(client_set.current_client(), Some(&origin));
    }

    #[test]
    fn directional_swap_does_not_change_history() {
        let mut client_set = stack_set();
        client_set.focus_client(&xid(1));
        let geometry = HashMap::from([
            (xid(1), Rect::new(0, 0, 100, 100)),
            (xid(2), Rect::new(100, 0, 100, 100)),
            (xid(3), Rect::new(200, 0, 100, 100)),
        ]);
        let mut history = DirectionalNavigationHistory::default();
        history.record(&client_set, xid(3), xid(1));

        client_set
            .swap_in_direction(Direction::Right, None, |client| {
                geometry.get(client).copied()
            })
            .expect("swap clients");

        assert_eq!(history.preferred_origin(&client_set, xid(1)), Some(xid(3)));
        assert_eq!(history.preferred_origin(&client_set, xid(2)), None);
    }

    #[test]
    fn changing_workspaces_clears_all_entries() {
        let mut client_set = stack_set();
        let mut history = DirectionalNavigationHistory::default();
        history.record(&client_set, xid(1), xid(2));

        client_set.pull_tag_to_screen("two");
        history.synchronize(&client_set);

        assert!(history.origins_by_destination.is_empty());
        assert_eq!(history.active_workspace_tag.as_deref(), Some("two"));
    }

    #[test]
    fn closed_clients_are_pruned_before_history_is_read() {
        let mut client_set = stack_set();
        let mut history = DirectionalNavigationHistory::default();
        history.record(&client_set, xid(1), xid(2));

        client_set.remove_client(&xid(1));

        assert_eq!(history.preferred_origin(&client_set, xid(2)), None);
        assert!(history.origins_by_destination.is_empty());
    }

    #[test]
    fn pruning_a_stale_entry_preserves_live_history() {
        let mut client_set = stack_set();
        let mut history = DirectionalNavigationHistory::default();
        history.record(&client_set, xid(1), xid(2));
        history.record(&client_set, xid(2), xid(3));

        client_set.remove_client(&xid(1));

        assert_eq!(history.preferred_origin(&client_set, xid(2)), None);
        assert_eq!(history.preferred_origin(&client_set, xid(3)), Some(xid(2)));
    }

    #[test]
    fn clients_moved_to_another_workspace_are_pruned() {
        let mut client_set = stack_set();
        let mut history = DirectionalNavigationHistory::default();
        history.record(&client_set, xid(1), xid(2));

        client_set.focus_client(&xid(1));
        client_set.move_focused_to_tag("two");

        assert_eq!(history.preferred_origin(&client_set, xid(2)), None);
        assert!(history.origins_by_destination.is_empty());
    }
}
