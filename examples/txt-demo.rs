//! Demo of the text rendering API
use penrose::{
    pure::geometry::Rect as PRect,
    x::{Atom, WinType, XConn},
    x11rb::RustConn,
};
use x11_draw::{Draw, Rect};

const X: u32 = 200;
const Y: u32 = 100;
const W: u32 = 600;
const H: u32 = 60;
const FONT: &str = "ProFont For Powerline:size=14";

fn main() -> anyhow::Result<()> {
    let conn = RustConn::new()?;
    // let w = conn.create_window(
    //     WinType::InputOutput(Atom::NetWindowTypeDock),
    //     PRect::new(X, Y, W, H),
    //     false,
    // )?;

    let mut drw = Draw::new(*conn.root(), W, H, FONT)?;
    // drw.add_colorscheme("primary", "#f2e5bc", "#282828")?;

    // let r = Rect {
    //     x: 0,
    //     y: 0,
    //     w: W,
    //     h: H,
    // };

    // drw.draw_text("    text is great", 20, r, false)?;
    // drw.flush_to(*w, r);
    // conn.map(w)?;

    // std::thread::sleep(std::time::Duration::from_secs(5));

    drw.show_font_match_for_chars("    text is great");

    Ok(())
}
