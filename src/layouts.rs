use penrose::{
    builtin::layout::{
        transformers::{Gaps, ReflectHorizontal, ReserveTop},
        MainAndStack, Monocle,
    },
    core::layout::Layout,
    pure::Stack,
    stack,
};

use crate::BAR_HEIGHT_PX;

pub fn layouts() -> Stack<Box<dyn Layout>> {
    let max_main = 1;
    let ratio = 0.6;
    let ratio_step = 0.1;
    let outer_px = 0;
    let inner_px = 0;

    stack!(
        MainAndStack::side(max_main, ratio, ratio_step),
        ReflectHorizontal::wrap(MainAndStack::side(max_main, ratio, ratio_step)),
        MainAndStack::bottom(max_main, ratio, ratio_step),
        Monocle::boxed()
    )
    .map(|layout| ReserveTop::wrap(Gaps::wrap(layout, outer_px, inner_px), BAR_HEIGHT_PX))
}
