use std::time::Duration;

use penrose::{util::spawn_for_output_with_args, x::XConn, Color};
use penrose_ui::{
    bar::widgets::{
        amixer_volume, battery_summary, current_date_and_time, wifi_network, ActiveWindowName,
        CurrentLayout, IntervalText, Widget, Workspaces,
    },
    Position, StatusBar, TextStyle,
};

use crate::{BAR_HEIGHT_PX, BLACK, BLUE, FONT, GREY, MAX_ACTIVE_WINDOW_CHARS, POINT_SIZE, WHITE};

pub fn status_bar<X: XConn>() -> penrose_ui::Result<StatusBar<X>> {
    let highlight: Color = BLUE.into();
    let empty_ws: Color = GREY.into();

    let style = TextStyle {
        fg: WHITE.into(),
        bg: Some(BLACK.into()),
        padding: (2, 2),
    };

    let padded_style = TextStyle {
        padding: (4, 2),
        ..style
    };

    StatusBar::try_new(
        Position::Top,
        BAR_HEIGHT_PX,
        style.bg.unwrap_or_else(|| 0x000000.into()),
        FONT,
        POINT_SIZE,
        vec![
            Box::new(Empty(100, false)),
            // Box::new(Text::new("                       ", style, false, true)),
            Box::new(Workspaces::new(style, highlight, empty_ws)),
            Box::new(CurrentLayout::new(style)),
            Box::new(ActiveWindowName::new(
                MAX_ACTIVE_WINDOW_CHARS,
                TextStyle {
                    bg: Some(highlight),
                    padding: (6, 4),
                    ..style
                },
                true,
                false,
            )),
            Box::new(current_weather_info(padded_style)),
            Box::new(wifi_network(padded_style)),
            // Box::new(battery_summary("BAT1", padded_style)),
            Box::new(battery_summary("BAT0", padded_style)),
            Box::new(amixer_volume("Master", padded_style)),
            Box::new(current_date_and_time(padded_style)),
        ],
    )
}

fn current_weather_info(style: TextStyle) -> IntervalText {
    IntervalText::new(style, get_weather_text, Duration::from_secs(60 * 5))
}

// Make a curl request to wttr.in to fetch the current weather information
// for our location.
fn get_weather_text() -> String {
    spawn_for_output_with_args("curl", &["-s", "http://wttr.in?format=%c%t"])
        .unwrap_or_default()
        .trim()
        .to_string()
}

struct Empty(u32, bool);

impl<X: XConn> Widget<X> for Empty {
    fn draw(
        &mut self,
        _ctx: &mut penrose_ui::Context<'_>,
        _screen: usize,
        _screen_has_focus: bool,
        _w: u32,
        _h: u32,
    ) -> penrose_ui::Result<()> {
        Ok(())
    }

    fn current_extent(
        &mut self,
        _ctx: &mut penrose_ui::Context<'_>,
        h: u32,
    ) -> penrose_ui::Result<(u32, u32)> {
        Ok((self.0, h))
    }

    fn require_draw(&self) -> bool {
        true
    }

    fn is_greedy(&self) -> bool {
        self.1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_weather_text_works() {
        let s = get_weather_text();
        assert!(!s.is_empty());
    }
}
