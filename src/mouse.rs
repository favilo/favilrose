use penrose::{
    builtin::actions::floating::{toggle_floating_focused, MouseDragHandler, MouseResizeHandler},
    core::bindings::{click_handler, MouseBindings, MouseState},
    map,
    x::XConn,
};

pub fn mouse_bindings<X>() -> MouseBindings<X>
where
    X: XConn + 'static,
{
    use penrose::core::bindings::{
        ModifierKey::{Meta, Shift},
        MouseButton::{Left, Middle, Right},
    };

    map! (
        map_keys: |(button, modifiers)| MouseState { button, modifiers };

        (Left, vec![Meta]) => MouseDragHandler::boxed_default(),
        (Right, vec![Meta]) => MouseResizeHandler::boxed_default(),
        (Middle, vec![Shift, Meta]) => click_handler(toggle_floating_focused()),
    )
}
