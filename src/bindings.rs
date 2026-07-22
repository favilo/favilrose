use std::collections::HashMap;

use color_eyre::eyre::Result;
use penrose::{
    builtin::{
        actions::{exit, floating::sink_focused, key_handler, modify_with, send_layout_message},
        layout::messages::{ExpandMain, IncMain, ShrinkMain},
    },
    core::{bindings::KeyEventHandler, State},
    pure::StackSet,
    util,
    x::{XConn, XConnExt},
    x11rb::RustConn,
};

use crate::{
    config::{Action, Command, PenroseConfig, Program},
    extensions::{
        directional_navigation_history::DirectionalNavigationHistory,
        stack_set::{Direction, StackSetExt},
    },
};

/// Build handlers for all configured keybindings.
///
/// # Errors
///
/// Returns an error if a configured handler cannot be constructed.
pub fn key_bindings(
    config: &PenroseConfig,
) -> Result<HashMap<String, Box<dyn KeyEventHandler<RustConn>>>> {
    config
        .keybindings
        .iter()
        .map(|(key, action)| Ok((key.as_ref().clone(), action_handler(*action, config)?)))
        .collect()
}

fn action_handler(
    action: Action,
    config: &PenroseConfig,
) -> Result<Box<dyn KeyEventHandler<RustConn>>> {
    Ok(match action {
        Action::FocusDown => modify_with(StackSet::focus_down),
        Action::FocusUp => modify_with(StackSet::focus_up),
        Action::SwapDown => modify_with(StackSet::swap_down),
        Action::SwapUp => modify_with(StackSet::swap_up),
        Action::KillFocused => modify_with(StackSet::kill_focused),
        Action::SinkFocused => sink_focused(),
        Action::ToggleTag => modify_with(StackSet::toggle_tag),
        Action::NextScreen => modify_with(StackSet::next_screen),
        Action::PreviousScreen => modify_with(StackSet::previous_screen),
        Action::NextLayout => modify_with(StackSet::next_layout),
        Action::PreviousLayout => modify_with(StackSet::previous_layout),
        Action::IncreaseMain => send_layout_message(|| IncMain(1)),
        Action::DecreaseMain => send_layout_message(|| IncMain(-1)),
        Action::ExpandMain => send_layout_message(|| ExpandMain),
        Action::ShrinkMain => send_layout_message(|| ShrinkMain),
        Action::LockAndSuspend => lock_and_suspend_handler(config)?,
        Action::Launcher => spawn_program(config.command(Program::Launcher)?),
        Action::Terminal => spawn_program(config.command(Program::Terminal)?),
        Action::Restart => exit(),
        Action::Quit => spawn_program(config.command(Program::Quit)?),
        Action::VolumeUp => spawn_program(config.command(Program::VolumeUp)?),
        Action::VolumeDown => spawn_program(config.command(Program::VolumeDown)?),
        Action::VolumeMute => spawn_program(config.command(Program::VolumeMute)?),
        Action::MediaPlayPause => spawn_program(config.command(Program::MediaPlayPause)?),
        Action::MediaNext => spawn_program(config.command(Program::MediaNext)?),
        Action::MediaPrevious => spawn_program(config.command(Program::MediaPrevious)?),
        Action::BrightnessUp => spawn_program(config.command(Program::BrightnessUp)?),
        Action::BrightnessDown => spawn_program(config.command(Program::BrightnessDown)?),
        Action::Screenshot => spawn_program(config.command(Program::Screenshot)?),
        Action::FocusTag(tag) => focus_tag_handler(tag),
        Action::MoveFocusTo(tag) => {
            key_handler(move |state: &mut State<RustConn>, x: &RustConn| {
                state.client_set.move_focused_to_tag(tag.to_string());
                x.refresh(state)
            })
        }
        Action::FocusDirection(direction) => focus_direction_handler(direction),
        Action::MoveDirection(direction) => move_direction_handler(direction),
    })
}

fn lock_and_suspend_handler(config: &PenroseConfig) -> Result<Box<dyn KeyEventHandler<RustConn>>> {
    let lock = config.command(Program::Lock)?;
    let suspend = config.command(Program::Suspend)?;

    Ok(key_handler(move |_, _| {
        util::spawn(lock.as_ref().clone())?;
        util::spawn(suspend.as_ref().clone())?;
        Ok(())
    }))
}

fn spawn_program(command: Command) -> Box<dyn KeyEventHandler<RustConn>> {
    key_handler(move |_, _| util::spawn(command.as_ref().clone()))
}

fn focus_tag_handler(tag: u8) -> Box<dyn KeyEventHandler<RustConn>> {
    key_handler(move |state: &mut State<RustConn>, x: &RustConn| {
        state.client_set.pull_tag_to_screen(tag.to_string());
        x.refresh(state)
    })
}

fn focus_direction_handler<X>(direction: Direction) -> Box<dyn KeyEventHandler<X>>
where
    X: XConn + 'static,
{
    key_handler(move |state: &mut State<X>, x: &X| {
        let origin = state.client_set.current_client().copied();
        let preferred = origin.and_then(|focused| {
            let history = state.extension_or_default::<DirectionalNavigationHistory>();
            let mut history = history.borrow_mut();
            history.preferred_origin(&state.client_set, focused)
        });
        let destination =
            state
                .client_set
                .focus_in_direction(direction, preferred.as_ref(), |client| {
                    x.client_geometry(*client)
                        .inspect_err(|error| {
                            tracing::debug!(?error, ?client, "read client geometry");
                        })
                        .ok()
                });

        if let (Some(origin), Some(destination)) = (origin, destination) {
            let history = state.extension_or_default::<DirectionalNavigationHistory>();
            history
                .borrow_mut()
                .record(&state.client_set, origin, destination);
            x.refresh(state)?;
        }

        Ok(())
    })
}

fn move_direction_handler<X>(direction: Direction) -> Box<dyn KeyEventHandler<X>>
where
    X: XConn + 'static,
{
    key_handler(move |state: &mut State<X>, x: &X| {
        let moved = state
            .client_set
            .swap_in_direction(direction, None, |client| {
                x.client_geometry(*client)
                    .inspect_err(|error| tracing::debug!(?error, ?client, "read client geometry"))
                    .ok()
            })
            .map_err(|error| penrose::Error::Custom(format!("{error:#}")))?;

        if moved.is_some() {
            x.refresh(state)?;
        }

        Ok(())
    })
}
