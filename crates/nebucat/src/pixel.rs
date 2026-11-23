use gpui_kit::*;
use nebucat_core::power::PowerAction;

/// Side of a glyph grid in cells.
const GLYPH_CELLS: usize = 11;
/// Logical pixels per glyph cell; 11 cells fill 22 of the 24px icon slot.
const GLYPH_CELL_PX: f32 = 2.;

/// One string per row; `#` is a filled cell.
pub type Glyph = [&'static str; GLYPH_CELLS];

const RESTART: Glyph = [
    "...#####.#.",
    "..#.....##.",
    ".#.....###.",
    "#..........",
    "#..........",
    "#.........#",
    "#.........#",
    "#.........#",
    ".#.......#.",
    "..#.....#..",
    "...#####...",
];

const SHUTDOWN: Glyph = [
    ".....#.....",
    "..#..#..#..",
    ".#...#...#.",
    "#....#....#",
    "#....#....#",
    "#.........#",
    "#.........#",
    "#.........#",
    ".#.......#.",
    "..#.....#..",
    "...#####...",
];

const SLEEP: Glyph = [
    "...##......",
    "..#.#......",
    ".#..#......",
    "#...#......",
    "#...#......",
    "#....#.....",
    "#.....#...#",
    "#......###.",
    ".#.......#.",
    "..#.....#..",
    "...#####...",
];

const HIBERNATE: Glyph = [
    ".....#.....",
    "...#.#.#...",
    "....###....",
    ".#...#...#.",
    "..#..#..#..",
    "###########",
    "..#..#..#..",
    ".#...#...#.",
    "....###....",
    "...#.#.#...",
    ".....#.....",
];

pub fn power_glyph(action: PowerAction) -> &'static Glyph {
    match action {
        PowerAction::Restart => &RESTART,
        PowerAction::Shutdown => &SHUTDOWN,
        PowerAction::Sleep => &SLEEP,
        PowerAction::Hibernate => &HIBERNATE,
    }
}

/// A one-color glyph sized to the row icon slot.
pub fn glyph(glyph: &'static Glyph, color: Hsla) -> impl IntoElement {
    let color = Rgba::from(color);
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            paint(bounds, GLYPH_CELLS, window, |x, y| {
                (glyph[y].as_bytes().get(x) == Some(&b'#')).then_some(color)
            })
        },
    )
    .size(px(GLYPH_CELLS as f32 * GLYPH_CELL_PX))
    .flex_none()
}

/// Paints an `n`×`n` grid over `bounds`, one quad per same-colored run of cells in a row.
pub fn paint(
    bounds: Bounds<Pixels>,
    n: usize,
    window: &mut Window,
    cell: impl Fn(usize, usize) -> Option<Rgba>,
) {
    // Cell edges land on whole device pixels, so cells stay sharp at fractional scale factors.
    let scale = window.scale_factor();
    let left = (f32::from(bounds.origin.x) * scale).round();
    let top = (f32::from(bounds.origin.y) * scale).round();
    let span = (f32::from(bounds.size.width) * scale).round();
    let edge = |i: usize| (i as f32 * span / n as f32).round();
    let logical = |device: f32| px(device / scale);

    for y in 0..n {
        let mut x = 0;
        while x < n {
            let color = cell(x, y);
            let start = x;
            while x < n && cell(x, y) == color {
                x += 1;
            }
            let Some(color) = color else { continue };
            let quad = Bounds::from_corners(
                point(logical(left + edge(start)), logical(top + edge(y))),
                point(logical(left + edge(x)), logical(top + edge(y + 1))),
            );
            window.paint_quad(fill(quad, color));
        }
    }
}
