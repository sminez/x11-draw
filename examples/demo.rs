//! Demo of the API
use penrose::{
    pure::geometry::Rect as PRect,
    x::{Atom, WinType, XConn},
    x11rb::RustConn,
};
use x11_draw::{Draw, Rect};

const X: u32 = 1500;
const Y: u32 = 100;
const W: u32 = 500;
const H: u32 = 60;

fn main() -> anyhow::Result<()> {
    let conn = RustConn::new()?;
    let w = conn.create_window(
        WinType::InputOutput(Atom::NetWindowTypeDock),
        PRect::new(X, Y, W, H),
        false,
    )?;

    let mut drw = Draw::new(*conn.root(), W, H);
    drw.set_fonts(&["ProFont For Powerline:size=10", "Iosevka Nerd Font:size=10"])?;
    drw.add_colorscheme("primary", "#f2e5bc", "#282828")?;
    drw.add_colorscheme("secondary", "#458588", "#b16286")?;

    let r = Rect {
        x: 0,
        y: 0,
        w: W,
        h: H,
    };

    for n in 0..4 {
        let scheme = if n % 2 == 0 { "primary" } else { "secondary" };
        let invert = n >= 2;
        drw.set_colorscheme(scheme)?;
        drw.fill_rect(r, invert)?;
        drw.flush_to(*w, r);
        conn.map(w)?;

        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    Ok(())
}
